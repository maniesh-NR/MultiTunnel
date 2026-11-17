//! Routing rule table: ordered first-match rules over inbound targets.
//!
//! CONTRACT (harness/CONTRACTS-S6.md "Expert B"):
//! - `RuleMatcher::{DomainSuffix, DomainKeyword, IpCidr, Any}` →
//!   `Action::{Proxy, Direct, Reject}` (types frozen in lib.rs).
//! - Ordered first-match; configurable default action when nothing matches
//!   (default `Proxy`).
//! - serde camelCase JSON for the rule list; domain rules match suffixes at
//!   LABEL boundaries — `evil-example.com` must NOT match suffix
//!   `example.com`, `www.example.com` must.
//!
//! IMPLEMENTATION NOTES (S6-B decisions, "pick one and document it"):
//! - Suffix matching is ASCII-case-insensitive; a leading '.' on the
//!   configured suffix is normalized away; equality matches; otherwise the
//!   suffix must be preceded by a '.' IN THE HOST (that is the label
//!   boundary). `Router` is also serde-able as
//!   `{"rules": [...], "defaultAction": "..."}` for config files.
//! - The router NEVER resolves DNS: domain rules only match `Host::Domain`
//!   targets and CIDR rules only match `Host::Ip` targets. Resolution, if
//!   any, is the outbounds' business (tokio's ToSocketAddrs). Documented
//!   consequence: a domain target with a CIDR-only rule table falls through
//!   to the default action.
//! - CIDR strings are parsed at MATCH time (rule tables are tiny, so the
//!   per-connection parse cost is negligible and [`RuleMatcher`] stays
//!   exactly the frozen serde shape). An unparseable CIDR matches nothing;
//!   `Router::new`/`with_default_action` log a `warn!` so config mistakes
//!   are visible but can never panic or refuse to start (fail-open).
//! - `DomainKeyword` is an ASCII-case-insensitive substring containment on
//!   the domain (clash-style semantics); an empty keyword matches every
//!   domain (and logs a warning at construction).
//! - Empty rules → every target takes the default action.

use ipnet::IpNet;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{Action, Host, Rule, RuleMatcher, TargetAddr};

/// An ordered routing table. Serialized as camelCase JSON per the
/// contract ("serde (camelCase JSON) for the rule list"), i.e.
/// `{"rules": [...], "defaultAction": "..."}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Router {
    /// Rules in priority order; the first match wins.
    pub rules: Vec<Rule>,
    /// Action when no rule matches (defaults to `Proxy` via `new`).
    pub default_action: Action,
}

fn default_proxy_action() -> Action {
    Action::Proxy
}

impl<'de> Deserialize<'de> for Router {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Manual impl so `defaultAction` can be omitted in JSON without
        // requiring `Action: Default` (the frozen Action type has none).
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire {
            rules: Vec<Rule>,
            #[serde(default = "default_proxy_action")]
            default_action: Action,
        }
        let w = Wire::deserialize(d)?;
        Ok(Router {
            rules: w.rules,
            default_action: w.default_action,
        })
    }
}

impl Router {
    /// New router with the contract default action (`Proxy`).
    pub fn new(rules: Vec<Rule>) -> Self {
        Self::with_default_action(rules, Action::Proxy)
    }

    /// New router with a custom fallback action.
    pub fn with_default_action(rules: Vec<Rule>, default_action: Action) -> Self {
        for rule in &rules {
            match &rule.matcher {
                RuleMatcher::IpCidr { cidr } if cidr.parse::<IpNet>().is_err() => {
                    tracing::warn!("router: unparseable CIDR '{cidr}' — rule matches nothing");
                }
                RuleMatcher::DomainKeyword { keyword } if keyword.is_empty() => {
                    tracing::warn!("router: empty DomainKeyword — matches every domain");
                }
                _ => {}
            }
        }
        Self {
            rules,
            default_action,
        }
    }

    /// First-match routing: the first rule whose matcher accepts `target`
    /// decides; otherwise the default action.
    pub fn route(&self, target: &TargetAddr) -> Action {
        for rule in &self.rules {
            if rule.matcher.matches(target) {
                return rule.action;
            }
        }
        self.default_action
    }
}

impl RuleMatcher {
    /// Does this matcher accept `target`? (Pure; see module docs for the
    /// domain/CIDR interaction rules.)
    pub fn matches(&self, target: &TargetAddr) -> bool {
        match self {
            RuleMatcher::DomainSuffix { suffix } => match &target.host {
                Host::Domain(d) => domain_suffix_match(d, suffix),
                Host::Ip(_) => false,
            },
            RuleMatcher::DomainKeyword { keyword } => match &target.host {
                Host::Domain(d) => d
                    .to_ascii_lowercase()
                    .contains(&keyword.to_ascii_lowercase()),
                Host::Ip(_) => false,
            },
            RuleMatcher::IpCidr { cidr } => match (&target.host, cidr.parse::<IpNet>()) {
                (Host::Ip(ip), Ok(net)) => net.contains(ip),
                _ => false,
            },
            RuleMatcher::Any => true,
        }
    }
}

/// Label-boundary suffix match (see module docs): case-insensitive, leading
/// dots normalized, equal hosts match, otherwise the character before the
/// suffix must be a '.'.
fn domain_suffix_match(host: &str, suffix: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let suffix = suffix.trim_start_matches('.').to_ascii_lowercase();
    if suffix.is_empty() || host.len() < suffix.len() {
        return false;
    }
    if !host.ends_with(&suffix) {
        return false;
    }
    host.len() == suffix.len() || host.as_bytes()[host.len() - suffix.len() - 1] == b'.'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(host: Host, port: u16) -> TargetAddr {
        TargetAddr { host, port }
    }

    fn domain(d: &str) -> TargetAddr {
        target(Host::Domain(d.into()), 443)
    }

    #[test]
    fn first_match_wins_in_order() {
        let router = Router::new(vec![
            Rule {
                matcher: RuleMatcher::DomainSuffix {
                    suffix: "example.org".into(),
                },
                action: Action::Direct,
            },
            Rule {
                matcher: RuleMatcher::Any,
                action: Action::Proxy,
            },
        ]);
        assert_eq!(router.route(&domain("www.example.org")), Action::Direct);
        assert_eq!(router.route(&domain("other.net")), Action::Proxy);
    }

    #[test]
    fn suffix_matches_at_label_boundaries_only() {
        let router = Router::new(vec![Rule {
            matcher: RuleMatcher::DomainSuffix {
                suffix: "example.com".into(),
            },
            action: Action::Direct,
        }]);
        // Must match: equal host, subdomain.
        assert_eq!(router.route(&domain("example.com")), Action::Direct);
        assert_eq!(router.route(&domain("www.example.com")), Action::Direct);
        assert_eq!(router.route(&domain("a.b.example.com")), Action::Direct);
        // Must NOT match: label-boundary lookalike.
        assert_eq!(
            router.route(&domain("evil-example.com")),
            Action::Proxy,
            "evil-example.com must not match suffix example.com"
        );
        assert_eq!(router.route(&domain("example.com.evil.net")), Action::Proxy);
    }

    #[test]
    fn suffix_is_case_insensitive_and_normalizes_leading_dot() {
        let router = Router::new(vec![Rule {
            matcher: RuleMatcher::DomainSuffix {
                suffix: ".Example.COM".into(),
            },
            action: Action::Direct,
        }]);
        assert_eq!(router.route(&domain("WWW.example.com")), Action::Direct);
        assert_eq!(router.route(&domain("EXAMPLE.com")), Action::Direct);
        // Empty suffix matches nothing.
        let empty = Router::new(vec![Rule {
            matcher: RuleMatcher::DomainSuffix {
                suffix: String::new(),
            },
            action: Action::Direct,
        }]);
        assert_eq!(empty.route(&domain("anything.net")), Action::Proxy);
    }

    #[test]
    fn keyword_matches_substring_case_insensitively() {
        let router = Router::new(vec![Rule {
            matcher: RuleMatcher::DomainKeyword {
                keyword: "Ads".into(),
            },
            action: Action::Reject,
        }]);
        assert_eq!(router.route(&domain("ads.example.net")), Action::Reject);
        assert_eq!(
            router.route(&domain("doubleclick.ADSYNDICATE.io")),
            Action::Reject
        );
        assert_eq!(router.route(&domain("clean.example.net")), Action::Proxy);
        // Keywords never match IP targets.
        let ip = target(Host::Ip("203.0.113.9".parse().unwrap()), 80);
        assert_eq!(router.route(&ip), Action::Proxy);
    }

    #[test]
    fn cidr_matches_v4_v6_and_never_domains() {
        let router = Router::new(vec![
            Rule {
                matcher: RuleMatcher::IpCidr {
                    cidr: "10.0.0.0/8".into(),
                },
                action: Action::Direct,
            },
            Rule {
                matcher: RuleMatcher::IpCidr {
                    cidr: "2001:db8::/32".into(),
                },
                action: Action::Reject,
            },
        ]);
        let v4 = target(Host::Ip("10.1.2.3".parse().unwrap()), 80);
        let v4_out = target(Host::Ip("192.0.2.55".parse().unwrap()), 80);
        let v6 = target(Host::Ip("2001:db8::9".parse().unwrap()), 80);
        assert_eq!(router.route(&v4), Action::Direct);
        assert_eq!(router.route(&v4_out), Action::Proxy);
        assert_eq!(router.route(&v6), Action::Reject);
        // Domain targets never hit CIDR rules → default.
        assert_eq!(router.route(&domain("example.net")), Action::Proxy);
    }

    #[test]
    fn default_action_is_configurable() {
        let router = Router::with_default_action(Vec::new(), Action::Direct);
        assert_eq!(router.route(&domain("anything.net")), Action::Direct);
        let fresh = Router::new(Vec::new());
        assert_eq!(fresh.route(&domain("anything.net")), Action::Proxy);
    }

    #[test]
    fn unparseable_cidr_matches_nothing_and_never_panics() {
        let router = Router::new(vec![Rule {
            matcher: RuleMatcher::IpCidr {
                cidr: "not-a-cidr".into(),
            },
            action: Action::Direct,
        }]);
        let ip = target(Host::Ip("10.0.0.1".parse().unwrap()), 80);
        assert_eq!(router.route(&ip), Action::Proxy);
    }

    #[test]
    fn serde_camelcase_roundtrip() {
        let json = r#"{
            "rules": [
                {"matcher": {"kind": "domainSuffix", "suffix": ".example.com"}, "action": "direct"},
                {"matcher": {"kind": "domainKeyword", "keyword": "ads"}, "action": "reject"},
                {"matcher": {"kind": "ipCidr", "cidr": "203.0.113.0/24"}, "action": "proxy"},
                {"matcher": {"kind": "any"}, "action": "proxy"}
            ],
            "defaultAction": "direct"
        }"#;
        let router: Router = serde_json::from_str(json).unwrap();
        assert_eq!(router.rules.len(), 4);
        assert_eq!(router.default_action, Action::Direct);
        assert_eq!(router.route(&domain("sub.example.com")), Action::Direct);
        assert_eq!(
            router.route(&target(Host::Ip("203.0.113.7".parse().unwrap()), 443)),
            Action::Proxy
        );
        // Round-trip through serialization keeps the same behavior.
        let re: Router = serde_json::from_str(&serde_json::to_string(&router).unwrap()).unwrap();
        assert_eq!(re, router);
    }
}
