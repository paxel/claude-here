//! The shipped toolchains. Each one is an image layer with a fixed build
//! order, the host caches it wants mounted, the egress domains it needs and the
//! language servers it provides. Enabling a toolchain is meant to be enough:
//! no further configuration, no user-written Dockerfile.

use anyhow::{Result, bail};

/// A host cache directory mounted into the container for a toolchain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cache {
    /// Key of the `[caches]` config override, also the name under
    /// `~/.config/claude_here/cache/` when `caches.isolated` is set.
    pub key: &'static str,
    /// Default host path, `~` expanded later.
    pub host: &'static str,
    /// Container target. Relative paths are resolved against the container
    /// home, absolute ones are used as they are.
    pub container: &'static str,
    /// Mount these subdirectories instead of the directory itself. Used where
    /// the parent holds image-owned content that must not be shadowed
    /// (`~/.cargo/bin`, the baked Android command line tools).
    pub subdirs: &'static [&'static str],
}

/// A language server declaration, handed to Claude Code through the bundled
/// plugin so the LSP tool works without the user installing anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lsp {
    pub name: &'static str,
    pub command: &'static str,
    pub args: &'static [&'static str],
    /// `(extension, language id)` pairs.
    pub extensions: &'static [(&'static str, &'static str)],
}

/// One shipped toolchain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Toolchain {
    pub name: &'static str,
    /// Extra names accepted on the command line (`--c` for `cpp`).
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    /// Build position. Lower builds first; alphabetical order would put
    /// dependants before what they depend on.
    pub order: u32,
    /// Toolchains pulled in automatically.
    pub implies: &'static [&'static str],
    pub dockerfile: &'static str,
    pub caches: &'static [Cache],
    /// Hosts this toolchain needs when the egress allowlist is on.
    pub domains: &'static [&'static str],
    pub lsp: &'static [Lsp],
    /// Host credential directories mounted when the cloud mode is not `none`.
    /// `(host path, container path relative to the container home)`.
    pub credentials: &'static [(&'static str, &'static str)],
}

const NODE: &str = include_str!("../images/toolchains/node.dockerfile");
const UV: &str = include_str!("../images/toolchains/uv.dockerfile");
const PYTHON: &str = include_str!("../images/toolchains/python.dockerfile");
const JVM: &str = include_str!("../images/toolchains/jvm.dockerfile");
const ANDROID: &str = include_str!("../images/toolchains/android.dockerfile");
const RUST: &str = include_str!("../images/toolchains/rust.dockerfile");
const GO: &str = include_str!("../images/toolchains/go.dockerfile");
const CPP: &str = include_str!("../images/toolchains/cpp.dockerfile");
const DART: &str = include_str!("../images/toolchains/dart.dockerfile");
const DOCS: &str = include_str!("../images/toolchains/docs.dockerfile");
const K8S: &str = include_str!("../images/toolchains/k8s.dockerfile");
const TERRAFORM: &str = include_str!("../images/toolchains/terraform.dockerfile");
const AWS: &str = include_str!("../images/toolchains/aws.dockerfile");
const GCLOUD: &str = include_str!("../images/toolchains/gcloud.dockerfile");
const AZURE: &str = include_str!("../images/toolchains/azure.dockerfile");

/// Hosts every session needs, independent of the enabled toolchains.
pub const BASE_DOMAINS: &[&str] = &["api.anthropic.com", "statsig.anthropic.com", "claude.ai"];

/// The shipped registry.
pub const TOOLCHAINS: &[Toolchain] = &[
    Toolchain {
        name: "node",
        aliases: &["npm", "js"],
        description: "Node.js, npm, pnpm, yarn, TypeScript",
        order: 10,
        implies: &[],
        dockerfile: NODE,
        caches: &[Cache {
            key: "npm",
            host: "~/.npm",
            container: ".npm",
            subdirs: &[],
        }],
        domains: &["deb.nodesource.com", "registry.npmjs.org", "nodejs.org"],
        lsp: &[Lsp {
            name: "typescript-language-server",
            command: "typescript-language-server",
            args: &["--stdio"],
            extensions: &[
                (".ts", "typescript"),
                (".tsx", "typescriptreact"),
                (".js", "javascript"),
                (".jsx", "javascriptreact"),
                (".mts", "typescript"),
                (".cts", "typescript"),
            ],
        }],
        credentials: &[],
    },
    Toolchain {
        name: "uv",
        aliases: &[],
        description: "uv/uvx, also how most MCP servers are launched",
        order: 15,
        implies: &[],
        dockerfile: UV,
        caches: &[Cache {
            key: "uv",
            host: "~/.cache/uv",
            container: ".cache/uv",
            subdirs: &[],
        }],
        domains: &[
            "astral.sh",
            "pypi.org",
            "files.pythonhosted.org",
            "github.com",
        ],
        lsp: &[],
        credentials: &[],
    },
    Toolchain {
        name: "python",
        aliases: &[],
        description: "poetry, ruff, mypy, uv, pyright",
        order: 20,
        implies: &["node", "uv"],
        dockerfile: PYTHON,
        caches: &[Cache {
            key: "pip",
            host: "~/.cache/pip",
            container: ".cache/pip",
            subdirs: &[],
        }],
        domains: &["pypi.org", "files.pythonhosted.org"],
        lsp: &[Lsp {
            name: "pyright",
            command: "pyright-langserver",
            args: &["--stdio"],
            extensions: &[(".py", "python"), (".pyi", "python")],
        }],
        credentials: &[],
    },
    Toolchain {
        name: "jvm",
        aliases: &[],
        description: "GraalVM CE 21, Maven, Gradle, kotlinc, jdtls",
        order: 30,
        implies: &[],
        dockerfile: JVM,
        caches: &[
            Cache {
                key: "m2",
                host: "~/.m2",
                container: ".m2",
                subdirs: &[],
            },
            Cache {
                key: "gradle",
                host: "~/.gradle",
                container: ".gradle",
                subdirs: &[],
            },
        ],
        domains: &[
            "repo.maven.apache.org",
            "repo1.maven.org",
            "plugins.gradle.org",
            "services.gradle.org",
            "download.eclipse.org",
            "github.com",
        ],
        lsp: &[Lsp {
            name: "jdtls",
            command: "jdtls",
            args: &[],
            extensions: &[(".java", "java")],
        }],
        credentials: &[],
    },
    Toolchain {
        name: "android",
        aliases: &[],
        description: "Android command line tools and platform-tools (no emulator)",
        order: 35,
        implies: &["jvm"],
        dockerfile: ANDROID,
        caches: &[Cache {
            key: "android",
            host: "~/Android/Sdk",
            container: "/opt/android-sdk",
            subdirs: &["platforms", "build-tools", "ndk"],
        }],
        domains: &["dl.google.com", "maven.google.com", "services.gradle.org"],
        lsp: &[],
        credentials: &[],
    },
    Toolchain {
        name: "rust",
        aliases: &[],
        description: "rustup stable, clippy, rustfmt, rust-analyzer",
        order: 40,
        implies: &[],
        dockerfile: RUST,
        caches: &[Cache {
            key: "cargo",
            host: "~/.cargo",
            container: ".cargo",
            subdirs: &["registry", "git"],
        }],
        domains: &[
            "static.rust-lang.org",
            "sh.rustup.rs",
            "crates.io",
            "index.crates.io",
            "static.crates.io",
        ],
        lsp: &[Lsp {
            name: "rust-analyzer",
            command: "rust-analyzer",
            args: &[],
            extensions: &[(".rs", "rust")],
        }],
        credentials: &[],
    },
    Toolchain {
        name: "go",
        aliases: &[],
        description: "Go toolchain and gopls",
        order: 45,
        implies: &[],
        dockerfile: GO,
        caches: &[Cache {
            key: "go",
            host: "~/go/pkg/mod",
            container: "go/pkg/mod",
            subdirs: &[],
        }],
        domains: &[
            "go.dev",
            "proxy.golang.org",
            "sum.golang.org",
            "storage.googleapis.com",
        ],
        lsp: &[Lsp {
            name: "gopls",
            command: "gopls",
            args: &[],
            extensions: &[(".go", "go")],
        }],
        credentials: &[],
    },
    Toolchain {
        name: "cpp",
        aliases: &["c"],
        description: "cmake, ninja, gdb, clang, clangd, valgrind, conan",
        order: 50,
        implies: &[],
        dockerfile: CPP,
        caches: &[Cache {
            key: "conan",
            host: "~/.conan2",
            container: ".conan2",
            subdirs: &[],
        }],
        domains: &["center.conan.io", "pypi.org", "files.pythonhosted.org"],
        lsp: &[Lsp {
            name: "clangd",
            command: "clangd",
            args: &[],
            extensions: &[
                (".c", "c"),
                (".h", "c"),
                (".cc", "cpp"),
                (".cpp", "cpp"),
                (".cxx", "cpp"),
                (".hpp", "cpp"),
                (".hxx", "cpp"),
            ],
        }],
        credentials: &[],
    },
    Toolchain {
        name: "dart",
        aliases: &["flutter"],
        description: "Flutter and Dart SDK with the Dart language server",
        order: 55,
        implies: &[],
        dockerfile: DART,
        caches: &[Cache {
            key: "pub",
            host: "~/.pub-cache",
            container: ".pub-cache",
            subdirs: &[],
        }],
        domains: &[
            "pub.dev",
            "storage.googleapis.com",
            "github.com",
            "dl.google.com",
        ],
        lsp: &[Lsp {
            name: "dart",
            command: "dart",
            args: &["language-server", "--protocol=lsp"],
            extensions: &[(".dart", "dart")],
        }],
        credentials: &[],
    },
    Toolchain {
        name: "docs",
        aliases: &[],
        description: "plantuml, d2, typst, pandoc (graphviz is in the base)",
        order: 60,
        implies: &[],
        dockerfile: DOCS,
        caches: &[],
        domains: &["github.com", "objects.githubusercontent.com"],
        lsp: &[],
        credentials: &[],
    },
    Toolchain {
        name: "k8s",
        aliases: &["kubernetes"],
        description: "kubectl, helm, kustomize",
        order: 70,
        implies: &[],
        dockerfile: K8S,
        caches: &[],
        domains: &[
            "dl.k8s.io",
            "get.helm.sh",
            "github.com",
            "storage.googleapis.com",
        ],
        lsp: &[],
        credentials: &[("~/.kube", ".kube")],
    },
    Toolchain {
        name: "terraform",
        aliases: &[],
        description: "terraform",
        order: 75,
        implies: &[],
        dockerfile: TERRAFORM,
        caches: &[],
        domains: &["releases.hashicorp.com", "registry.terraform.io"],
        lsp: &[],
        credentials: &[],
    },
    Toolchain {
        name: "aws",
        aliases: &[],
        description: "AWS CLI v2",
        order: 80,
        implies: &[],
        dockerfile: AWS,
        caches: &[],
        domains: &["awscli.amazonaws.com"],
        lsp: &[],
        credentials: &[("~/.aws", ".aws")],
    },
    Toolchain {
        name: "gcloud",
        aliases: &[],
        description: "Google Cloud CLI (large)",
        order: 85,
        implies: &[],
        dockerfile: GCLOUD,
        caches: &[],
        domains: &["dl.google.com", "googleapis.com"],
        lsp: &[],
        credentials: &[("~/.config/gcloud", ".config/gcloud")],
    },
    Toolchain {
        name: "azure",
        aliases: &["az"],
        description: "Azure CLI",
        order: 90,
        implies: &[],
        dockerfile: AZURE,
        caches: &[],
        domains: &["pypi.org", "files.pythonhosted.org"],
        lsp: &[],
        credentials: &[("~/.azure", ".azure")],
    },
];

/// Look a toolchain up by its name or one of its aliases.
pub fn find(name: &str) -> Option<&'static Toolchain> {
    TOOLCHAINS
        .iter()
        .find(|t| t.name == name || t.aliases.contains(&name))
}

/// Canonical name for a name or alias.
pub fn canonical(name: &str) -> Option<&'static str> {
    find(name).map(|t| t.name)
}

/// Expand implications, drop duplicates, sort into build order.
///
/// The result is independent of the order the names were given, so a
/// combination always produces one tag and one hash.
pub fn resolve(names: &[String]) -> Result<Vec<&'static Toolchain>> {
    let mut out: Vec<&'static Toolchain> = Vec::new();
    let mut queue: Vec<String> = names.to_vec();
    while let Some(name) = queue.pop() {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let Some(t) = find(name) else {
            bail!(
                "unknown toolchain '{name}'; known: {}",
                TOOLCHAINS
                    .iter()
                    .map(|t| t.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        };
        if out.iter().any(|o| o.name == t.name) {
            continue;
        }
        out.push(t);
        queue.extend(t.implies.iter().map(|s| (*s).to_string()));
    }
    out.sort_by(|a, b| a.order.cmp(&b.order).then(a.name.cmp(b.name)));
    Ok(out)
}

/// Names in build order, for tags, session info and messages.
pub fn names(chain: &[&Toolchain]) -> Vec<String> {
    chain.iter().map(|t| t.name.to_string()).collect()
}

/// Every domain the chain needs, plus the ones every session needs.
pub fn domains(chain: &[&Toolchain]) -> Vec<String> {
    let mut v: Vec<String> = BASE_DOMAINS.iter().map(|d| (*d).to_string()).collect();
    for t in chain {
        for d in t.domains {
            if !v.iter().any(|x| x == d) {
                v.push((*d).to_string());
            }
        }
    }
    v
}

/// Credential directories the chain wants when the cloud mode allows them.
pub fn credentials(chain: &[&Toolchain]) -> Vec<(&'static str, &'static str)> {
    chain
        .iter()
        .flat_map(|t| t.credentials.iter().copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(names: &[&str]) -> Vec<&'static str> {
        let v: Vec<String> = names.iter().map(|s| (*s).to_string()).collect();
        resolve(&v)
            .unwrap_or_default()
            .iter()
            .map(|t| t.name)
            .collect()
    }

    #[test]
    fn order_is_independent_of_input_order() {
        assert_eq!(resolved(&["rust", "jvm"]), resolved(&["jvm", "rust"]));
        assert_eq!(resolved(&["rust", "jvm"]), vec!["jvm", "rust"]);
    }

    #[test]
    fn implications_are_pulled_in_before_their_dependant() {
        assert_eq!(resolved(&["android"]), vec!["jvm", "android"]);
        assert_eq!(resolved(&["python"]), vec!["node", "uv", "python"]);
    }

    #[test]
    fn duplicates_and_aliases_collapse() {
        assert_eq!(resolved(&["node", "npm", "js"]), vec!["node"]);
        assert_eq!(resolved(&["c"]), vec!["cpp"]);
        assert_eq!(canonical("flutter"), Some("dart"));
        assert_eq!(canonical("nope"), None);
    }

    #[test]
    fn unknown_name_is_an_error() {
        assert!(resolve(&["zig".to_string()]).is_err());
    }

    #[test]
    fn registry_is_consistent() {
        for t in TOOLCHAINS {
            assert!(!t.dockerfile.is_empty(), "{}", t.name);
            assert!(
                t.dockerfile.contains("FROM ${BASE}"),
                "{} must build on BASE",
                t.name
            );
            for i in t.implies {
                let Some(implied) = find(i) else {
                    panic!("{} implies unknown toolchain {i}", t.name)
                };
                assert!(
                    implied.order < t.order,
                    "{} must build after {}",
                    t.name,
                    implied.name
                );
            }
            assert!(
                TOOLCHAINS.iter().filter(|o| o.order == t.order).count() == 1,
                "duplicate order {}",
                t.order
            );
        }
    }

    #[test]
    fn only_cloud_toolchains_carry_credentials() {
        for t in TOOLCHAINS {
            let expected = matches!(t.name, "k8s" | "aws" | "gcloud" | "azure");
            assert_eq!(
                !t.credentials.is_empty(),
                expected,
                "{} credentials",
                t.name
            );
        }
        let chain = resolve(&["k8s".to_string(), "aws".to_string()]).unwrap_or_default();
        assert_eq!(
            credentials(&chain),
            vec![("~/.kube", ".kube"), ("~/.aws", ".aws")]
        );
    }

    #[test]
    fn domains_include_base_and_toolchain_hosts() {
        let chain = resolve(&["rust".to_string()]).unwrap_or_default();
        let d = domains(&chain);
        assert!(d.contains(&"api.anthropic.com".to_string()));
        assert!(d.contains(&"static.crates.io".to_string()));
    }
}
