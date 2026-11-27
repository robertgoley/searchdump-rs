//! Cluster TLS options and secrets (L4-001), and how they reach children.

use crate::cli::Args;
use anyhow::{Result, bail};
use searchdump_rs::http::tls::TlsOptions;

/// L4-001: a secret flag value (the key passphrase): `Debug` never shows it.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Secret(pub(crate) String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

pub(crate) fn parse_secret(s: &str) -> std::result::Result<Secret, String> {
    Ok(Secret(s.to_string()))
}

/// The cluster's side for searchdump: a dump reads it (`--input-*`), a load writes it
/// (`--output-*`).
pub(crate) type Side = searchdump_rs::endpoint::Direction;

/// L4-001 (1.4, C11): TLS settings for the one cluster of a dump or load: used by the parent's
/// own requests and passed to every child. CA/cert/key paths go on the child's command line;
/// the key passphrase only through the child's environment (Robert, t253u).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ClusterTls {
    pub(crate) ca: Option<String>,
    pub(crate) cert: Option<String>,
    pub(crate) key: Option<String>,
    pub(crate) pass: Option<Secret>,
    /// `--insecure`, or `NODE_TLS_REJECT_UNAUTHORIZED=0` (see `node_insecure`).
    pub(crate) insecure: bool,
    /// 1.4 (t255u): `NODE_TLS_REJECT_UNAUTHORIZED=0` in the environment, as searchdump and Node
    /// elasticdump honour it: verification off for the parent's requests and every child.
    pub(crate) node_insecure: bool,
}

/// The variable Node (and searchdump) read: `0` turns certificate verification off.
pub(crate) const NODE_TLS_ENV: &str = "NODE_TLS_REJECT_UNAUTHORIZED";

impl ClusterTls {
    /// `--X` wins over `--{side}-X` (searchdump's order); the passphrase then falls back to
    /// `$SEARCHDUMP_TLS_PASS` and `$SEARCHDUMP_{SIDE}_TLS_PASS`. A flag for the other side is an
    /// error, since that side is a directory and the flag would silently do nothing.
    pub(crate) fn resolve(
        args: &Args,
        side: Side,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Self> {
        let (other, mine, theirs) = match side {
            Side::Input => (
                "output",
                [&args.input_ca, &args.input_cert, &args.input_key],
                [&args.output_ca, &args.output_cert, &args.output_key],
            ),
            Side::Output => (
                "input",
                [&args.output_ca, &args.output_cert, &args.output_key],
                [&args.input_ca, &args.input_cert, &args.input_key],
            ),
        };
        let (my_pass, their_pass) = match side {
            Side::Input => (&args.input_pass, &args.output_pass),
            Side::Output => (&args.output_pass, &args.input_pass),
        };
        for (name, v) in ["ca", "cert", "key"].iter().zip(theirs) {
            if v.is_some() {
                bail!(
                    "--{other}-{name} names the {other} cluster, but with --direction {} the cluster is the {}; use --{name} or --{}-{name}",
                    if side == Side::Input { "dump" } else { "load" },
                    side.as_str(),
                    side.as_str()
                );
            }
        }
        if their_pass.is_some() {
            bail!(
                "--{other}-pass names the {other} cluster, but the cluster is the {}; use --pass, --{}-pass or $SEARCHDUMP_TLS_PASS",
                side.as_str(),
                side.as_str()
            );
        }
        let pick = |shared: &Option<String>, own: &Option<String>| shared.clone().or(own.clone());
        let tls = ClusterTls {
            ca: pick(&args.ca, mine[0]),
            cert: pick(&args.cert, mine[1]),
            key: pick(&args.key, mine[2]),
            pass: args
                .pass
                .clone()
                .or(my_pass.clone())
                .or_else(|| {
                    searchdump_rs::envnames::lookup(env, searchdump_rs::config::TLS_PASS_ENV)
                        .filter(|v| !v.is_empty())
                        .map(Secret)
                })
                .or_else(|| {
                    searchdump_rs::envnames::lookup(env, &searchdump_rs::config::tls_pass_env(side))
                        .filter(|v| !v.is_empty())
                        .map(Secret)
                }),
            insecure: args.insecure || env(NODE_TLS_ENV).as_deref() == Some("0"),
            node_insecure: env(NODE_TLS_ENV).as_deref() == Some("0"),
        };
        if tls.cert.is_some() != tls.key.is_some() {
            bail!(
                "--cert and --key (or --{s}-cert/--{s}-key) must be given together",
                s = side.as_str()
            );
        }
        Ok(tls)
    }

    /// Whether any TLS flag of the cluster was given (copy has no cluster and refuses them).
    pub(crate) fn any_flag(args: &Args) -> bool {
        [
            &args.ca,
            &args.cert,
            &args.key,
            &args.input_ca,
            &args.input_cert,
            &args.input_key,
            &args.output_ca,
            &args.output_cert,
            &args.output_key,
        ]
        .iter()
        .any(|v| v.is_some())
            || args.pass.is_some()
            || args.input_pass.is_some()
            || args.output_pass.is_some()
    }

    /// The parent's own client settings.
    pub(crate) fn options(&self) -> TlsOptions {
        TlsOptions {
            ca: self.ca.clone().map(Into::into),
            cert: self.cert.clone().map(Into::into),
            key: self.key.clone().map(Into::into),
            pass: self.pass.as_ref().map(|p| p.0.clone()),
            insecure: self.insecure,
        }
    }

    /// Command-line flags for a child (no secret in them). `--insecure` also when only
    /// `NODE_TLS_REJECT_UNAUTHORIZED=0` turned verification off (the child inherits the variable
    /// too; the flag makes it explicit, whatever the child does with its environment).
    pub(crate) fn child_flags(&self, side: Side) -> Vec<String> {
        let d = side.as_str();
        let mut v = vec![];
        if self.insecure {
            v.push("--insecure".to_string());
        }
        if let Some(ca) = &self.ca {
            v.push(format!("--{d}-ca={ca}"));
        }
        if let (Some(cert), Some(key)) = (&self.cert, &self.key) {
            v.push(format!("--{d}-cert={cert}"));
            v.push(format!("--{d}-key={key}"));
            v.push("--tlsAuth".to_string());
        }
        v
    }

    /// Environment for a child: the passphrase as `$SEARCHDUMP_{SIDE}_TLS_PASS`, and the
    /// variables that would win over it (the shared one and the old `ELASTICDUMP_*` names)
    /// removed, so the child uses exactly the passphrase the parent resolved.
    pub(crate) fn child_env(&self, side: Side) -> (Vec<(String, String)>, Vec<String>) {
        let Some(pass) = &self.pass else {
            return (vec![], vec![]);
        };
        let own = searchdump_rs::config::tls_pass_env(side);
        let shared = searchdump_rs::config::TLS_PASS_ENV.to_string();
        let mut remove = vec![shared.clone()];
        remove.extend(searchdump_rs::envnames::old_name(&shared));
        remove.extend(searchdump_rs::envnames::old_name(&own));
        (vec![(own, pass.0.clone())], remove)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------ L4-001: TLS
    fn tls_args(extra: &[&str]) -> Args {
        use clap::Parser;
        let mut v = vec!["m", "--input=https://h:9200", "--output=/tmp/x"];
        v.extend_from_slice(extra);
        crate::Args::try_parse_from(v).unwrap()
    }

    #[test]
    fn l4_001_cluster_tls_flags_and_precedence() {
        use super::{ClusterTls, Secret, Side};
        let none = |_: &str| None;
        let a = tls_args(&[
            "--ca=shared.pem",
            "--input-ca=in.pem",
            "--input-cert=c.pem",
            "--input-key=k.pem",
        ]);
        let t = ClusterTls::resolve(&a, Side::Input, &none).unwrap();
        assert_eq!(
            t.ca.as_deref(),
            Some("shared.pem"),
            "--ca wins like searchdump"
        );
        assert_eq!(t.cert.as_deref(), Some("c.pem"));
        assert_eq!(
            t.child_flags(Side::Input),
            vec![
                "--input-ca=shared.pem",
                "--input-cert=c.pem",
                "--input-key=k.pem",
                "--tlsAuth"
            ]
        );
        // a load passes the same settings as --output-*
        let a = tls_args(&["--ca=ca.pem", "--insecure"]);
        let t = ClusterTls::resolve(&a, Side::Output, &none).unwrap();
        assert_eq!(
            t.child_flags(Side::Output),
            vec!["--insecure", "--output-ca=ca.pem"]
        );
        let o = t.options();
        assert_eq!(o.ca.unwrap().to_str(), Some("ca.pem"));
        assert!(o.insecure && o.pass.is_none());
        // flags for the other side, or a cert without a key, are refused
        let a = tls_args(&["--output-ca=x.pem"]);
        let e = ClusterTls::resolve(&a, Side::Input, &none).unwrap_err();
        assert!(format!("{e}").contains("--output-ca"), "{e}");
        let a = tls_args(&["--output-pass=FAKE-tls-pp-0"]);
        let e = ClusterTls::resolve(&a, Side::Input, &none).unwrap_err();
        assert!(!format!("{e}").contains("FAKE"), "{e}");
        let a = tls_args(&["--cert=c.pem"]);
        assert!(ClusterTls::resolve(&a, Side::Input, &none).is_err());
        // no flags: nothing added
        let t = ClusterTls::resolve(&tls_args(&[]), Side::Input, &none).unwrap();
        assert!(t.child_flags(Side::Input).is_empty());
        assert_eq!(t.child_env(Side::Input), (vec![], vec![]));
        // t255u: NODE_TLS_REJECT_UNAUTHORIZED=0 turns verification off for the parent and the
        // children (as --insecure; the children inherit the variable too); other values do not
        for (v, off) in [("0", true), ("1", false), ("", false), ("false", false)] {
            let env = |k: &str| (k == NODE_TLS_ENV).then(|| v.to_string());
            let t = ClusterTls::resolve(&tls_args(&["--ca=ca.pem"]), Side::Input, &env).unwrap();
            assert_eq!(t.node_insecure, off, "{v:?}");
            assert_eq!(t.options().insecure, off, "{v:?}");
            assert_eq!(
                t.child_flags(Side::Input)
                    .contains(&"--insecure".to_string()),
                off,
                "{v:?}"
            );
            assert_eq!(
                t.child_env(Side::Input),
                (vec![], vec![]),
                "the variable is inherited, not removed"
            );
        }
        assert!(!ClusterTls::any_flag(&tls_args(&["--insecure"])));
        assert!(ClusterTls::any_flag(&tls_args(&["--input-key=k"])));
        assert!(ClusterTls::any_flag(&tls_args(&["--pass=x"])));
        let _ = Secret("x".into());
    }

    #[test]
    fn l4_001_passphrase_goes_to_children_only_through_the_environment() {
        use super::{ClusterTls, Side};
        let pp = "FAKE-tls-pp-1";
        let env = |k: &str| match k {
            "SEARCHDUMP_TLS_PASS" => Some(pp.to_string()),
            _ => None,
        };
        let a = tls_args(&["--cert=c.pem", "--key=k.pem"]);
        let t = ClusterTls::resolve(&a, Side::Output, &env).unwrap();
        assert_eq!(t.options().pass.as_deref(), Some(pp));
        let flags = t.child_flags(Side::Output);
        assert!(!flags.iter().any(|f| f.contains(pp)), "{flags:?}");
        let (set, remove) = t.child_env(Side::Output);
        assert_eq!(
            set,
            vec![("SEARCHDUMP_OUTPUT_TLS_PASS".to_string(), pp.to_string())]
        );
        assert!(remove.contains(&"SEARCHDUMP_TLS_PASS".to_string()));
        assert!(remove.contains(&"ELASTICDUMP_TLS_PASS".to_string()));
        assert!(remove.contains(&"ELASTICDUMP_OUTPUT_TLS_PASS".to_string()));
        // Debug of the args and the settings never shows it
        let a = tls_args(&["--pass", pp]);
        assert!(!format!("{a:?}").contains(pp));
        let t = ClusterTls::resolve(&a, Side::Input, &|_| None).unwrap();
        assert!(!format!("{t:?}").contains(pp));
        // flag beats env; side env used when nothing else is set; empty env is unset
        let side_env = |k: &str| match k {
            "SEARCHDUMP_INPUT_TLS_PASS" => Some("FAKE-side".to_string()),
            "SEARCHDUMP_TLS_PASS" => Some(String::new()),
            _ => None,
        };
        let t = ClusterTls::resolve(&tls_args(&[]), Side::Input, &side_env).unwrap();
        assert_eq!(t.pass.unwrap().0, "FAKE-side");
        let t =
            ClusterTls::resolve(&tls_args(&["--input-pass=FAKE-flag"]), Side::Input, &env).unwrap();
        assert_eq!(t.pass.unwrap().0, "FAKE-flag");
    }
}
