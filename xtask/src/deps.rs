//! `cargo xtask deps`: dependency direction and player-only licenses (MS-02, PROV-08).
//!
//! Checks the workspace against `xtask/layers.toml`:
//! - every workspace crate is listed, and its normal and build dependencies on workspace crates
//!   are on its allow-list;
//! - restricted (chip and core) crates use only the allowed external crates;
//! - no package whose license needs a player-only license is reachable from a crate other than
//!   the player. cargo-deny exceptions are keyed by dependency, not by consumer, so this check is
//!   what keeps player-only licenses out of library crates.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result};
use cargo_metadata::{CargoOpt, DependencyKind, MetadataCommand, PackageId};

use crate::{
    finding::Finding,
    provenance::{ALLOWED_LICENSES, license_permitted},
};

const LAYERS: &str = "xtask/layers.toml";

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Layers {
    policy: Policy,
    allow: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Policy {
    restricted: Vec<String>,
    restricted_external: Vec<String>,
    player_crates: Vec<String>,
    player_only_licenses: Vec<String>,
}

/// A workspace crate's direct normal and build dependencies.
#[derive(Debug, Default)]
struct Direct {
    workspace: BTreeSet<String>,
    external: BTreeSet<String>,
}

pub(crate) fn run(root: &Path) -> Result<Vec<Finding>> {
    let text =
        fs::read_to_string(root.join(LAYERS)).with_context(|| format!("reading {LAYERS}"))?;
    let layers = toml::from_str::<Layers>(&text).with_context(|| format!("parsing {LAYERS}"))?;
    let metadata = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .features(CargoOpt::AllFeatures)
        .exec()
        .context("running cargo metadata")?;

    let members = metadata
        .workspace_packages()
        .into_iter()
        .map(|package| (package.name.to_string(), package))
        .collect::<BTreeMap<_, _>>();
    let mut direct = BTreeMap::new();
    for (name, package) in &members {
        let mut dependencies = Direct::default();
        for dependency in &package.dependencies {
            if !matches!(
                dependency.kind,
                DependencyKind::Normal | DependencyKind::Build
            ) {
                continue;
            }
            let dependency_name = dependency.name.clone();
            if dependency.path.is_some() && members.contains_key(&dependency_name) {
                dependencies.workspace.insert(dependency_name);
            } else {
                dependencies.external.insert(dependency_name);
            }
        }
        direct.insert(name.clone(), dependencies);
    }
    let mut findings = check_layers(&layers, &direct);

    let licenses = metadata
        .packages
        .iter()
        .map(|package| {
            (
                package.id.clone(),
                (
                    package.name.to_string(),
                    package.license.clone().unwrap_or_default(),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut edges = BTreeMap::<PackageId, Vec<PackageId>>::new();
    if let Some(resolve) = &metadata.resolve {
        for node in &resolve.nodes {
            let targets = node
                .deps
                .iter()
                .filter(|dependency| {
                    dependency.dep_kinds.iter().any(|info| {
                        matches!(info.kind, DependencyKind::Normal | DependencyKind::Build)
                    })
                })
                .map(|dependency| dependency.pkg.clone())
                .collect::<Vec<_>>();
            edges.insert(node.id.clone(), targets);
        }
    }
    for (name, package) in &members {
        if layers.policy.player_crates.contains(name) {
            continue;
        }
        for reached in reachable(&package.id, &edges) {
            if let Some((reached_name, license)) = licenses.get(&reached)
                && needs_player_only_license(license, &layers.policy.player_only_licenses)
            {
                findings.push(Finding::new(
                    LAYERS,
                    0,
                    format!("`{name}` reaches `{reached_name}` ({license}), which is allowed only for the player (PROV-08)"),
                ));
            }
        }
    }
    Ok(findings)
}

fn check_layers(layers: &Layers, direct: &BTreeMap<String, Direct>) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (name, dependencies) in direct {
        let Some(allowed) = layers.allow.get(name) else {
            findings.push(Finding::new(
                LAYERS,
                0,
                format!("workspace crate `{name}` is not listed under [allow]"),
            ));
            continue;
        };
        for dependency in &dependencies.workspace {
            if !allowed.contains(dependency) {
                findings.push(Finding::new(
                    LAYERS,
                    0,
                    format!("`{name}` depends on `{dependency}`, which its [allow] entry does not permit (MS-02)"),
                ));
            }
        }
        if layers.policy.restricted.contains(name) {
            for dependency in &dependencies.external {
                if !layers.policy.restricted_external.contains(dependency) {
                    findings.push(Finding::new(
                        LAYERS,
                        0,
                        format!("chip/core crate `{name}` depends on `{dependency}`; chip and core crates may use only {} without an ADR", layers.policy.restricted_external.join(", ")),
                    ));
                }
            }
        }
    }
    for name in layers.allow.keys() {
        if !direct.contains_key(name) {
            findings.push(Finding::new(
                LAYERS,
                0,
                format!("[allow] lists `{name}`, which is not a workspace crate"),
            ));
        }
    }
    findings
}

fn reachable(
    start: &PackageId,
    edges: &BTreeMap<PackageId, Vec<PackageId>>,
) -> BTreeSet<PackageId> {
    let mut seen = BTreeSet::new();
    let mut stack = edges.get(start).cloned().unwrap_or_default();
    while let Some(next) = stack.pop() {
        if seen.insert(next.clone())
            && let Some(targets) = edges.get(&next)
        {
            stack.extend(targets.iter().cloned());
        }
    }
    seen
}

/// Whether `license` can only be met with a player-only license: it is not permitted under the
/// library list, but is under the library list plus the player-only licenses.
fn needs_player_only_license(license: &str, player_only: &[String]) -> bool {
    let library = |term: &str| ALLOWED_LICENSES.contains(&term);
    let player = |term: &str| library(term) || player_only.iter().any(|allowed| allowed == term);
    !license_permitted(license, &library) && license_permitted(license, &player)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layers() -> Layers {
        toml::from_str::<Layers>(
            r#"
            [policy]
            restricted = ["chip"]
            restricted-external = ["serde"]
            player-crates = ["player"]
            player-only-licenses = ["MPL-2.0"]

            [allow]
            api = []
            chip = ["api"]
            player = ["api", "chip"]
            "#,
        )
        .unwrap()
    }

    fn direct(entries: &[(&str, &[&str], &[&str])]) -> BTreeMap<String, Direct> {
        entries
            .iter()
            .map(|(name, workspace, external)| {
                let dependencies = Direct {
                    workspace: workspace
                        .iter()
                        .map(|item| String::from(*item))
                        .collect::<BTreeSet<_>>(),
                    external: external
                        .iter()
                        .map(|item| String::from(*item))
                        .collect::<BTreeSet<_>>(),
                };
                (String::from(*name), dependencies)
            })
            .collect::<BTreeMap<_, _>>()
    }

    #[test]
    fn allowed_graph_passes() {
        let graph = direct(&[
            ("api", &[], &["serde"]),
            ("chip", &["api"], &["serde"]),
            ("player", &["api", "chip"], &["winit"]),
        ]);
        assert_eq!(check_layers(&layers(), &graph), Vec::<Finding>::new());
    }

    #[test]
    fn violations_are_reported() {
        let graph = direct(&[
            ("api", &["chip"], &[]),
            ("chip", &["api"], &["rand"]),
            ("player", &[], &[]),
            ("stray", &[], &[]),
        ]);
        let messages = check_layers(&layers(), &graph)
            .into_iter()
            .map(|finding| finding.message)
            .collect::<Vec<_>>();
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert!(messages[0].contains("`api` depends on `chip`"));
        assert!(messages[1].contains("`rand`"));
        assert!(messages[2].contains("`stray`"));
    }

    #[test]
    fn player_only_license_detection() {
        let player_only = [String::from("MPL-2.0")];
        assert!(needs_player_only_license("MPL-2.0", &player_only));
        assert!(!needs_player_only_license("MIT OR MPL-2.0", &player_only));
        assert!(!needs_player_only_license("MIT", &player_only));
        assert!(!needs_player_only_license("GPL-3.0", &player_only));
    }
}
