//! capa-style declarative capability rules (the idea from mandiant/capa, Apache-2.0, re-implemented here
//! in a much smaller form): YAML rules over `api / import / string / regex / section / export /
//! characteristic` leaves combined with `and / or / not / count`. Every rule carries `examples` that are
//! executed by `cargo test` and by `nexus-reverse rules --selftest`: a rule with no passing positive AND
//! negative example is a bug.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Expr {
    pub and: Option<Vec<Expr>>,
    pub or: Option<Vec<Expr>>,
    pub not: Option<Box<Expr>>,
    pub count: Option<CountExpr>,
    pub api: Option<String>,
    pub import: Option<String>,
    pub string: Option<String>,
    pub regex: Option<String>,
    pub section: Option<String>,
    pub export: Option<String>,
    pub characteristic: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountExpr {
    pub n: usize,
    pub of: Vec<Expr>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Example {
    pub expect: bool,
    #[serde(default)]
    pub apis: Vec<String>,
    #[serde(default)]
    pub dlls: Vec<String>,
    #[serde(default)]
    pub strings: Vec<String>,
    #[serde(default)]
    pub sections: Vec<String>,
    #[serde(default)]
    pub exports: Vec<String>,
    #[serde(default)]
    pub chars: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleDef {
    pub name: String,
    pub namespace: String,
    pub severity: Severity,
    #[serde(default)]
    pub attack: Vec<String>,
    pub description: String,
    pub features: Expr,
    #[serde(default)]
    pub examples: Vec<Example>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleFile {
    rules: Vec<RuleDef>,
}

/// What the rules look at. Built from the analysis (or, in self-tests, from an `Example`).
#[derive(Debug, Clone, Default)]
pub struct FactBag {
    /// lowercase API name -> original spelling (for citing evidence)
    pub apis: HashMap<String, String>,
    pub dlls: HashSet<String>,
    /// all strings (original case) - matched case-insensitively
    pub strings: Vec<String>,
    pub sections: HashSet<String>,
    pub exports: HashSet<String>,
    pub chars: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum FactKind {
    Api,
    Dll,
    Str,
    Section,
    Export,
    Char,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchedFact {
    pub kind: FactKind,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleHit {
    pub name: String,
    pub namespace: String,
    pub severity: Severity,
    pub attack: Vec<String>,
    pub description: String,
    pub facts: Vec<MatchedFact>,
    /// Filled in by `analyze` once the facts are linked to evidence IDs.
    pub evidence_ids: Vec<String>,
}

#[derive(Debug)]
pub struct CompiledRule {
    pub def: RuleDef,
    regexes: HashMap<String, Regex>,
}

#[derive(Debug)]
pub struct RuleSet {
    pub rules: Vec<CompiledRule>,
}

fn collect_regexes(e: &Expr, out: &mut HashMap<String, Regex>) -> Result<(), String> {
    if let Some(p) = &e.regex {
        if !out.contains_key(p) {
            out.insert(p.clone(), Regex::new(&format!("(?i){p}")).map_err(|x| format!("bad regex {p:?}: {x}"))?);
        }
    }
    for list in [&e.and, &e.or].into_iter().flatten() {
        for c in list {
            collect_regexes(c, out)?;
        }
    }
    if let Some(n) = &e.not {
        collect_regexes(n, out)?;
    }
    if let Some(c) = &e.count {
        for x in &c.of {
            collect_regexes(x, out)?;
        }
    }
    Ok(())
}

fn leaf_count(e: &Expr) -> usize {
    [e.and.is_some(), e.or.is_some(), e.not.is_some(), e.count.is_some(), e.api.is_some(), e.import.is_some(),
     e.string.is_some(), e.regex.is_some(), e.section.is_some(), e.export.is_some(), e.characteristic.is_some()]
        .iter()
        .filter(|&&b| b)
        .count()
}

fn validate_expr(e: &Expr) -> Result<(), String> {
    if leaf_count(e) != 1 {
        return Err("each expression must set exactly one of and/or/not/count/api/import/string/regex/section/export/characteristic".into());
    }
    for list in [&e.and, &e.or].into_iter().flatten() {
        if list.is_empty() {
            return Err("and/or lists must not be empty".into());
        }
        for c in list {
            validate_expr(c)?;
        }
    }
    if let Some(n) = &e.not {
        validate_expr(n)?;
    }
    if let Some(c) = &e.count {
        if c.n == 0 || c.n > c.of.len() {
            return Err(format!("count n={} must be between 1 and {}", c.n, c.of.len()));
        }
        for x in &c.of {
            validate_expr(x)?;
        }
    }
    Ok(())
}

impl RuleSet {
    pub fn from_yaml_sources(sources: &[&str]) -> Result<RuleSet, String> {
        let mut rules = Vec::new();
        let mut names = HashSet::new();
        for (i, src) in sources.iter().enumerate() {
            let file: RuleFile = serde_yaml::from_str(src).map_err(|e| format!("rule file #{i}: {e}"))?;
            for def in file.rules {
                if !names.insert(def.name.clone()) {
                    return Err(format!("duplicate rule name {:?}", def.name));
                }
                validate_expr(&def.features).map_err(|e| format!("rule {:?}: {e}", def.name))?;
                let mut regexes = HashMap::new();
                collect_regexes(&def.features, &mut regexes).map_err(|e| format!("rule {:?}: {e}", def.name))?;
                rules.push(CompiledRule { def, regexes });
            }
        }
        Ok(RuleSet { rules })
    }

    /// The rules shipped with NEXUS (embedded at compile time).
    pub fn builtin() -> Result<RuleSet, String> {
        RuleSet::from_yaml_sources(&[
            include_str!("../rules/injection.yaml"),
            include_str!("../rules/persistence.yaml"),
            include_str!("../rules/network.yaml"),
            include_str!("../rules/collection.yaml"),
            include_str!("../rules/evasion.yaml"),
            include_str!("../rules/host.yaml"),
        ])
    }

    pub fn evaluate(&self, bag: &FactBag) -> Vec<RuleHit> {
        let mut hits = Vec::new();
        for r in &self.rules {
            let mut facts = Vec::new();
            if eval(&r.def.features, bag, &r.regexes, &mut facts) {
                facts.dedup();
                hits.push(RuleHit {
                    name: r.def.name.clone(),
                    namespace: r.def.namespace.clone(),
                    severity: r.def.severity,
                    attack: r.def.attack.clone(),
                    description: r.def.description.clone(),
                    facts,
                    evidence_ids: Vec::new(),
                });
            }
        }
        hits.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.name.cmp(&b.name)));
        hits
    }
}

/// Shared builtin rule set (parsed once). `Err` carries the load error so callers can surface it.
pub fn builtin() -> &'static Result<RuleSet, String> {
    static R: OnceLock<Result<RuleSet, String>> = OnceLock::new();
    R.get_or_init(RuleSet::builtin)
}

fn api_match(bag: &FactBag, name: &str) -> Option<String> {
    let n = name.to_ascii_lowercase();
    for cand in [n.clone(), format!("{n}a"), format!("{n}w"), format!("{n}exa"), format!("{n}exw")] {
        if let Some(orig) = bag.apis.get(&cand) {
            return Some(orig.clone());
        }
    }
    None
}

fn eval(e: &Expr, bag: &FactBag, rx: &HashMap<String, Regex>, out: &mut Vec<MatchedFact>) -> bool {
    if let Some(list) = &e.and {
        let mut tmp = Vec::new();
        for c in list {
            if !eval(c, bag, rx, &mut tmp) {
                return false;
            }
        }
        out.extend(tmp);
        return true;
    }
    if let Some(list) = &e.or {
        let mut any = false;
        for c in list {
            let mut tmp = Vec::new();
            if eval(c, bag, rx, &mut tmp) {
                any = true;
                out.extend(tmp);
            }
        }
        return any;
    }
    if let Some(n) = &e.not {
        let mut tmp = Vec::new();
        return !eval(n, bag, rx, &mut tmp);
    }
    if let Some(c) = &e.count {
        let mut ok = 0;
        let mut tmp = Vec::new();
        for x in &c.of {
            let mut one = Vec::new();
            if eval(x, bag, rx, &mut one) {
                ok += 1;
                tmp.extend(one);
            }
        }
        if ok >= c.n {
            out.extend(tmp);
            return true;
        }
        return false;
    }
    if let Some(a) = &e.api {
        return match api_match(bag, a) {
            Some(orig) => {
                out.push(MatchedFact { kind: FactKind::Api, value: orig });
                true
            }
            None => false,
        };
    }
    if let Some(d) = &e.import {
        let d = d.to_ascii_lowercase();
        let d = if d.ends_with(".dll") || d.ends_with(".exe") || d.ends_with(".sys") { d } else { format!("{d}.dll") };
        if bag.dlls.contains(&d) {
            out.push(MatchedFact { kind: FactKind::Dll, value: d });
            return true;
        }
        return false;
    }
    if let Some(s) = &e.string {
        let want = s.to_ascii_lowercase();
        if let Some(hit) = bag.strings.iter().find(|x| x.to_ascii_lowercase().contains(&want)) {
            out.push(MatchedFact { kind: FactKind::Str, value: hit.clone() });
            return true;
        }
        return false;
    }
    if let Some(p) = &e.regex {
        if let Some(re) = rx.get(p) {
            if let Some(hit) = bag.strings.iter().find(|x| re.is_match(x)) {
                out.push(MatchedFact { kind: FactKind::Str, value: hit.clone() });
                return true;
            }
        }
        return false;
    }
    if let Some(s) = &e.section {
        let s = s.to_ascii_lowercase();
        if bag.sections.contains(&s) {
            out.push(MatchedFact { kind: FactKind::Section, value: s });
            return true;
        }
        return false;
    }
    if let Some(x) = &e.export {
        let x = x.to_ascii_lowercase();
        if bag.exports.contains(&x) {
            out.push(MatchedFact { kind: FactKind::Export, value: x });
            return true;
        }
        return false;
    }
    if let Some(c) = &e.characteristic {
        if bag.chars.contains(c) {
            out.push(MatchedFact { kind: FactKind::Char, value: c.clone() });
            return true;
        }
        return false;
    }
    false
}

impl Example {
    pub fn to_bag(&self) -> FactBag {
        let mut b = FactBag::default();
        for a in &self.apis {
            b.apis.insert(a.to_ascii_lowercase(), a.clone());
        }
        b.dlls = self.dlls.iter().map(|d| d.to_ascii_lowercase()).collect();
        b.strings = self.strings.clone();
        b.sections = self.sections.iter().map(|s| s.to_ascii_lowercase()).collect();
        b.exports = self.exports.iter().map(|s| s.to_ascii_lowercase()).collect();
        b.chars = self.chars.iter().cloned().collect();
        b
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SelfTestReport {
    pub rules: usize,
    pub positive_examples: usize,
    pub negative_examples: usize,
    pub failures: Vec<String>,
}

/// Run every rule's examples against the rule set (all rules, so a negative example must not fire the
/// rule under test; positive examples must).
pub fn selftest(set: &RuleSet) -> SelfTestReport {
    let mut rep = SelfTestReport { rules: set.rules.len(), ..Default::default() };
    for r in &set.rules {
        let pos = r.def.examples.iter().filter(|e| e.expect).count();
        let neg = r.def.examples.iter().filter(|e| !e.expect).count();
        rep.positive_examples += pos;
        rep.negative_examples += neg;
        if pos == 0 || neg == 0 {
            rep.failures.push(format!("{}: needs at least one positive and one negative example (has {pos}/{neg})", r.def.name));
        }
        for (i, ex) in r.def.examples.iter().enumerate() {
            let mut facts = Vec::new();
            let fired = eval(&r.def.features, &ex.to_bag(), &r.regexes, &mut facts);
            if fired != ex.expect {
                rep.failures.push(format!("{}: example #{i} expected {} but the rule {}", r.def.name, ex.expect, if fired { "fired" } else { "did not fire" }));
            }
        }
    }
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(y: &str) -> RuleSet {
        RuleSet::from_yaml_sources(&[y]).unwrap()
    }

    #[test]
    fn builtin_rules_load_and_every_example_passes() {
        let s = builtin().as_ref().expect("builtin rules must load");
        let rep = selftest(s);
        assert!(rep.failures.is_empty(), "{:#?}", rep.failures);
        assert!(rep.rules >= 30, "expected a meaningful rule set, got {}", rep.rules);
        assert!(rep.positive_examples >= rep.rules && rep.negative_examples >= rep.rules);
    }

    #[test]
    fn builtin_rules_have_attack_ids_on_every_non_info_rule() {
        for r in &builtin().as_ref().unwrap().rules {
            if r.def.severity != Severity::Info {
                assert!(!r.def.attack.is_empty(), "{} has no ATT&CK mapping", r.def.name);
                for a in &r.def.attack {
                    assert!(Regex::new(r"^T\d{4}(\.\d{3})? .+").unwrap().is_match(a), "{}: bad ATT&CK entry {a:?}", r.def.name);
                }
            }
        }
    }

    #[test]
    fn combinators_and_leaves() {
        let s = set(r#"
rules:
  - name: t
    namespace: test
    severity: low
    description: d
    features:
      and:
        - or: [{api: Foo}, {api: Bar}]
        - not: {api: Evil}
        - count: {n: 2, of: [{string: aaa}, {string: bbb}, {regex: 'c+d'}]}
        - characteristic: is_dll
    examples:
      - {expect: true, apis: [FooW], strings: [AAA1, xbbbx], chars: [is_dll]}
      - {expect: true, apis: [Bar], strings: [aaa, ccd], chars: [is_dll]}
      - {expect: false, apis: [Bar, Evil], strings: [aaa, bbb], chars: [is_dll]}
      - {expect: false, apis: [Bar], strings: [aaa], chars: [is_dll]}
"#);
        assert!(selftest(&s).failures.is_empty(), "{:?}", selftest(&s).failures);
    }

    #[test]
    fn api_matching_handles_a_w_and_ex_variants_but_not_prefixes() {
        let mut bag = FactBag::default();
        bag.apis.insert("createprocessw".into(), "CreateProcessW".into());
        bag.apis.insert("regsetvalueexa".into(), "RegSetValueExA".into());
        assert_eq!(api_match(&bag, "CreateProcess").as_deref(), Some("CreateProcessW"));
        assert_eq!(api_match(&bag, "RegSetValue").as_deref(), Some("RegSetValueExA"));
        assert!(api_match(&bag, "CreateProc").is_none());
        assert!(api_match(&bag, "CreateProcessInternal").is_none());
    }

    #[test]
    fn bad_rule_files_are_rejected_with_a_reason() {
        let bad = [
            "rules: [{name: a, namespace: n, severity: low, description: d, features: {api: A, string: b}}]", // two leaves
            "rules: [{name: a, namespace: n, severity: low, description: d, features: {and: []}}]",
            "rules: [{name: a, namespace: n, severity: low, description: d, features: {count: {n: 3, of: [{api: A}]}}}]",
            "rules: [{name: a, namespace: n, severity: low, description: d, features: {regex: '('}}]",
            "rules: [{name: a, namespace: n, severity: nope, description: d, features: {api: A}}]",
            "rules: [{name: a, namespace: n, severity: low, description: d, features: {api: A}, bogus: 1}]",
        ];
        for y in bad {
            assert!(RuleSet::from_yaml_sources(&[y]).is_err(), "{y}");
        }
        let dup = "rules: [{name: a, namespace: n, severity: low, description: d, features: {api: A}},{name: a, namespace: n, severity: low, description: d, features: {api: B}}]";
        assert!(RuleSet::from_yaml_sources(&[dup]).unwrap_err().contains("duplicate"));
    }

    #[test]
    fn selftest_reports_missing_negative_examples_and_wrong_expectations() {
        let s = set("rules: [{name: a, namespace: n, severity: low, description: d, features: {api: A}, examples: [{expect: true, apis: [A]}]}]");
        assert_eq!(selftest(&s).failures.len(), 1);
        let s2 = set("rules: [{name: a, namespace: n, severity: low, description: d, features: {api: A}, examples: [{expect: true, apis: [B]}, {expect: false, apis: [B]}]}]");
        assert_eq!(selftest(&s2).failures.len(), 1);
    }

    #[test]
    fn hits_are_sorted_by_severity_and_carry_matched_facts() {
        let s = set(r#"
rules:
  - {name: low one, namespace: n, severity: low, description: d, features: {api: A}, examples: [{expect: true, apis: [A]}, {expect: false}]}
  - {name: high one, namespace: n, severity: high, description: d, features: {api: A}, examples: [{expect: true, apis: [A]}, {expect: false}]}
"#);
        let mut bag = FactBag::default();
        bag.apis.insert("a".into(), "A".into());
        let hits = s.evaluate(&bag);
        assert_eq!(hits.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), ["high one", "low one"]);
        assert_eq!(hits[0].facts, vec![MatchedFact { kind: FactKind::Api, value: "A".into() }]);
    }
}
