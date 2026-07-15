//! Open the parent archive + Ext sidecar, verify the fingerprint, and gather the
//! denominators every fraction needs (design §2).
//!
//! Split in two: [`OpenedArchive`] is the expensive, one-time part (mmap the
//! archive, verify the fingerprint), done once per process -- or, for `serve`
//! mode, once per server startup, then shared via `Arc` across every request.
//! [`Ctx::for_bbox`] is the cheap, pure, per-request part (bbox clip +
//! denominators for one specific bbox), so a long-lived server can compute a
//! fresh bbox clip per request without reopening the archive.

use crate::bbox::BboxClip;
use crate::freshness;
use anyhow::{anyhow, Context as _};
use osmflat::{FileResourceStorage, Osm};
use osmflat_ext::query::Bbox;
use osmflat_ext::taginfo::TaginfoQuery;
use osmflat_ext::{Ext, ExtArchive};
use std::path::Path;
use std::sync::Arc;

/// Parent vector lengths used as fraction denominators. These are
/// sentinel-trimmed counts as the reader sees them — the same values the ext
/// fingerprint records — so they line up with how the sidecar counted objects.
///
/// When a `--bbox` is active these are the *bbox-filtered* totals (entities
/// overlapping the box), so fractions stay "share of what's in view" rather
/// than "share of the whole archive".
#[derive(Copy, Clone, Debug)]
pub struct Totals {
    pub nodes: u64,
    pub ways: u64,
    pub relations: u64,
    pub objects: u64,
}

impl Totals {
    fn of(parent: &Osm) -> Self {
        let nodes = parent.nodes().len() as u64;
        let ways = parent.ways().len() as u64;
        let relations = parent.relations().len() as u64;
        Totals {
            nodes,
            ways,
            relations,
            objects: nodes + ways + relations,
        }
    }

    fn of_clip(clip: &BboxClip) -> Self {
        let nodes = clip.totals.nodes;
        let ways = clip.totals.ways;
        let relations = clip.totals.relations;
        Totals {
            nodes,
            ways,
            relations,
            objects: nodes + ways + relations,
        }
    }
}

/// The opened + fingerprint-verified archive. Building this is the expensive
/// part (mmap the archive/sidecar, verify the fingerprint); cloning it is one
/// `Arc` bump + one `String` clone, so `serve` mode can open it once at
/// startup and share it across every request.
#[derive(Clone)]
pub struct OpenedArchive {
    archive: Arc<ExtArchive>,
    pub data_until: String,
}

impl OpenedArchive {
    /// Open both archives from filesystem paths and verify the sidecar
    /// matches.
    pub fn open(archive: &Path, ext: &Path) -> anyhow::Result<Self> {
        let parent = Osm::open(FileResourceStorage::new(archive))
            .with_context(|| format!("opening parent archive {}", archive.display()))?;
        let sidecar = Ext::open(FileResourceStorage::new(ext))
            .with_context(|| format!("opening Ext sidecar {}", ext.display()))?;

        let data_until = freshness::data_until(&parent, archive);

        let archive = ExtArchive::open(parent, sidecar)
            .map_err(|m| anyhow!("{m}"))
            .context("sidecar does not match this parent archive")?;

        Ok(OpenedArchive {
            archive: Arc::new(archive),
            data_until,
        })
    }

    /// Wrap an already-opened, fingerprint-verified archive. Test-only entry
    /// point (the CLI/serve path is [`Self::open`]).
    #[cfg(test)]
    pub(crate) fn from_archive(archive: ExtArchive, data_until: String) -> Self {
        OpenedArchive {
            archive: Arc::new(archive),
            data_until,
        }
    }
}

/// Everything an endpoint needs: the opened+verified archive, the denominators,
/// and the resolved `data_until` timestamp.
pub struct Ctx {
    archive: Arc<ExtArchive>,
    pub totals: Totals,
    pub data_until: String,
    /// The raw `--bbox` argument, if any. Endpoints use this as a cheap
    /// "bbox mode is active" signal.
    pub bbox: Option<Bbox>,
    /// Precomputed spatial ranges for [`Self::bbox`], reused across every
    /// value/key merge-join in a request.
    pub bbox_clip: Option<BboxClip>,
}

impl Ctx {
    /// Open both archives from CLI paths and build a `Ctx` for one bbox in a
    /// single call -- the CLI's one-shot-process convenience. `serve` mode
    /// instead opens once via [`OpenedArchive::open`] and calls
    /// [`Self::for_bbox`] fresh per request.
    pub fn open(
        archive: &Option<std::path::PathBuf>,
        ext: &Option<std::path::PathBuf>,
        bbox: Option<Bbox>,
    ) -> anyhow::Result<Self> {
        let archive = archive
            .as_deref()
            .ok_or_else(|| anyhow!("missing --archive (parent osmflat archive directory)"))?;
        let ext = ext
            .as_deref()
            .ok_or_else(|| anyhow!("missing --ext (Ext sidecar directory)"))?;
        let opened = OpenedArchive::open(archive, ext)?;
        Ok(Self::for_bbox(&opened, bbox))
    }

    /// The cheap, pure, per-request part: builds the bbox clip and
    /// denominators for one bbox against an already-opened archive. No I/O --
    /// just in-memory/mmap spatial range computation, `O(candidates in bbox)`.
    pub fn for_bbox(opened: &OpenedArchive, bbox: Option<Bbox>) -> Self {
        let bbox_clip = bbox.map(|bbox| BboxClip::new(opened.archive.parent(), bbox));
        let totals = match &bbox_clip {
            Some(clip) => Totals::of_clip(clip),
            None => Totals::of(opened.archive.parent()),
        };
        Ctx {
            archive: opened.archive.clone(),
            totals,
            data_until: opened.data_until.clone(),
            bbox,
            bbox_clip,
        }
    }

    /// Wrap an already-opened, fingerprint-verified archive with no bbox clip.
    /// Kept for the existing (pre-bbox) test call sites.
    #[cfg(test)]
    pub(crate) fn from_archive(archive: ExtArchive, data_until: String) -> Self {
        Self::from_archive_with_bbox(archive, data_until, None)
    }

    /// Wrap an already-opened, fingerprint-verified in-memory archive with a
    /// bbox clip. Test-only entry point.
    #[cfg(test)]
    pub(crate) fn from_archive_with_bbox(
        archive: ExtArchive,
        data_until: String,
        bbox: Option<Bbox>,
    ) -> Self {
        Self::for_bbox(&OpenedArchive::from_archive(archive, data_until), bbox)
    }

    /// The taginfo query layer, or an error if the sidecar was built without
    /// `--taginfo`.
    pub fn taginfo(&self) -> anyhow::Result<TaginfoQuery<'_>> {
        self.archive.taginfo().ok_or_else(|| {
            anyhow!("sidecar has no taginfo index (rebuild with `osmflat-extc --taginfo`)")
        })
    }

    /// Whether the sidecar has a taginfo index at all -- a whole-archive
    /// misconfiguration, not a per-query condition (unlike an unknown key/tag
    /// or a missing `--combinations` index, both of which just yield empty
    /// results). `serve` mode uses this to return 503 instead of an empty 200.
    pub fn has_taginfo(&self) -> bool {
        self.archive.taginfo().is_some()
    }
}

/// `count / total`, rounded to 4 decimal places to match taginfo's fraction
/// precision (it rounds to 4 dp; JSON then drops trailing zeros, so `0.2200`
/// prints as `0.22`). Guards against a zero denominator on an empty archive.
pub fn fraction(count: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let f = count as f64 / total as f64;
    (f * 10_000.0).round() / 10_000.0
}
