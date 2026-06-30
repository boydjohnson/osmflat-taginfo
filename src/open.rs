//! Open the parent archive + Ext sidecar, verify the fingerprint, and gather the
//! denominators every fraction needs (design §2).

use crate::freshness;
use anyhow::{anyhow, Context as _};
use osmflat::{FileResourceStorage, Osm};
use osmflat_ext::taginfo::TaginfoQuery;
use osmflat_ext::{Ext, ExtArchive};
use std::path::Path;

/// Parent vector lengths used as fraction denominators. These are
/// sentinel-trimmed counts as the reader sees them — the same values the ext
/// fingerprint records — so they line up with how the sidecar counted objects.
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
}

/// Everything an endpoint needs: the opened+verified archive, the denominators,
/// and the resolved `data_until` timestamp.
pub struct Ctx {
    archive: ExtArchive,
    pub totals: Totals,
    pub data_until: String,
}

impl Ctx {
    /// Open both archives from CLI paths and verify the sidecar matches.
    pub fn open(
        archive: &Option<std::path::PathBuf>,
        ext: &Option<std::path::PathBuf>,
    ) -> anyhow::Result<Self> {
        let archive = archive
            .as_deref()
            .ok_or_else(|| anyhow!("missing --archive (parent osmflat archive directory)"))?;
        let ext = ext
            .as_deref()
            .ok_or_else(|| anyhow!("missing --ext (Ext sidecar directory)"))?;
        Self::open_paths(archive, ext)
    }

    fn open_paths(archive: &Path, ext: &Path) -> anyhow::Result<Self> {
        let parent = Osm::open(FileResourceStorage::new(archive))
            .with_context(|| format!("opening parent archive {}", archive.display()))?;
        let sidecar = Ext::open(FileResourceStorage::new(ext))
            .with_context(|| format!("opening Ext sidecar {}", ext.display()))?;

        let data_until = freshness::data_until(&parent, archive);

        let archive = ExtArchive::open(parent, sidecar)
            .map_err(|m| anyhow!("{m}"))
            .context("sidecar does not match this parent archive")?;

        Ok(Self::from_archive(archive, data_until))
    }

    /// Wrap an already-opened, fingerprint-verified archive. Computes the
    /// fraction denominators from the parent. The construction path the tests
    /// use (with an in-memory archive); `open` is the CLI path.
    pub(crate) fn from_archive(archive: ExtArchive, data_until: String) -> Self {
        let totals = Totals::of(archive.parent());
        Ctx {
            archive,
            totals,
            data_until,
        }
    }

    /// The taginfo query layer, or an error if the sidecar was built without
    /// `--taginfo`.
    pub fn taginfo(&self) -> anyhow::Result<TaginfoQuery<'_>> {
        self.archive.taginfo().ok_or_else(|| {
            anyhow!("sidecar has no taginfo index (rebuild with `osmflat-extc --taginfo`)")
        })
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
