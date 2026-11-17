//! Held-out evaluation of a fine-tuned GGUF: generate replies to a fixed set
//! of prompts, then score them.
//!
//! A project keeps its eval set in `eval/`:
//!
//! ```text
//! eval_inputs.json   [EvalItem]
//! scoring.json       ScoringRules (optional; defaults otherwise)
//! results/<model>.json  [{"id": "<item id>#<sample>", "reply": "..."}]
//! ```
//!
//! The file shapes are the ones the earlier Python scorers read, so existing
//! projects load unchanged and their result files can be scored here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use fancy_regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

/// One held-out prompt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalItem {
    pub id: String,
    /// Groups items in the scorecard, e.g. `identity`, `identity-nosys`,
    /// `general`, `facts`. Kinds ending in `-nosys` are asked with no system
    /// prompt.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub user: String,
    /// What to show for the prompt when `user` carries host context around it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Patterns the reply must all match.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expect: Vec<String>,
    /// Patterns the reply must not match.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forbid: Vec<String>,
    /// The reply should contain [`ScoringRules::intro_sentence`] verbatim.
    #[serde(default)]
    pub intro: bool,
}

impl EvalItem {
    /// The system prompt this item is asked with. Kinds ending in `-nosys` are
    /// asked with none, and the scorer files them under the no-system columns,
    /// so generation and scoring both read it from here.
    pub fn system_prompt(&self) -> Option<&str> {
        if self.kind.ends_with("-nosys") {
            None
        } else {
            self.system.as_deref()
        }
    }

    /// The prompt as a person would read it.
    pub fn display(&self) -> &str {
        self.message.as_deref().unwrap_or(&self.user)
    }
}

/// One generated reply. `id` is `<item id>#<sample>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplyRow {
    pub id: String,
    pub reply: String,
}

/// Project-specific scoring. Every pattern uses Python-style regex syntax
/// (lookaround is allowed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScoringRules {
    /// The assistant's name. An identity reply holds when it contains this and
    /// claims no vendor. `None` skips the identity column.
    pub name: Option<String>,
    /// A name the model must not be called, e.g. a mangled version of `name`.
    pub wrong_names: Vec<String>,
    /// Vendors and models the assistant must not claim to be.
    pub vendor: String,
    /// A negation within the five words before a vendor name turns a claim
    /// into a denial ("I'm not ChatGPT").
    pub negation: String,
    pub identity_kinds: Vec<String>,
    /// Kinds counted for mean reply length.
    pub general_kinds: Vec<String>,
    /// Spellings forbidden in every reply, e.g. a misspelt product name.
    pub misspell: Option<String>,
    /// The exact sentence `intro` items must contain.
    pub intro_sentence: Option<String>,
}

pub const DEFAULT_VENDOR: &str = r"(?i)qwen|alibaba|tongyi|openai|chatgpt|gpt-|claude|anthropic|gemini|google|meta\b|llama|deepseek|mistral|copilot";
pub const DEFAULT_NEGATION: &str =
    r"(?i)\b(not|no|never|isn't|aren't|am not|can't|cannot|bukan|tidak)\b|n't|n’t";

impl Default for ScoringRules {
    fn default() -> Self {
        Self {
            name: None,
            wrong_names: Vec::new(),
            vendor: DEFAULT_VENDOR.to_string(),
            negation: DEFAULT_NEGATION.to_string(),
            identity_kinds: vec!["identity".into(), "identity-nosys".into()],
            general_kinds: vec!["general".into()],
            misspell: None,
            intro_sentence: None,
        }
    }
}

/// Generation settings. The defaults match the mobile config the Onde apps use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sampling {
    pub temperature: f64,
    pub top_p: f64,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub max_tokens: usize,
}

impl Default for Sampling {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: 0.95,
            frequency_penalty: None,
            presence_penalty: None,
            max_tokens: 256,
        }
    }
}

/// Paths of a project's eval files.
pub struct EvalDir {
    pub dir: PathBuf,
}

impl EvalDir {
    pub fn new(project_dir: &Path) -> Self {
        Self {
            dir: project_dir.join("eval"),
        }
    }
    pub fn inputs(&self) -> PathBuf {
        self.dir.join("eval_inputs.json")
    }
    pub fn rules(&self) -> PathBuf {
        self.dir.join("scoring.json")
    }
    pub fn results(&self) -> PathBuf {
        self.dir.join("results")
    }
    pub fn result(&self, model: &str) -> PathBuf {
        self.results().join(format!("{model}.json"))
    }
}

/// Read and check an eval set: unique ids, a kind and a prompt on every item,
/// and patterns that compile.
pub fn load_items(path: &Path) -> Result<Vec<EvalItem>> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let items: Vec<EvalItem> =
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
    anyhow::ensure!(!items.is_empty(), "{} has no items", path.display());
    let mut seen = std::collections::HashSet::new();
    for item in &items {
        anyhow::ensure!(
            !item.id.is_empty() && !item.id.contains('#'),
            "item id {:?} must be non-empty and contain no '#'",
            item.id
        );
        anyhow::ensure!(seen.insert(&item.id), "item id {:?} appears twice", item.id);
        anyhow::ensure!(!item.kind.is_empty(), "item {} has no kind", item.id);
        anyhow::ensure!(
            !item.user.trim().is_empty(),
            "item {} has an empty prompt",
            item.id
        );
        anyhow::ensure!(
            !(item.kind.ends_with("-nosys") && item.system.is_some()),
            "item {} is a {:?} item but has a system prompt; remove one",
            item.id,
            item.kind
        );
        for p in item.expect.iter().chain(&item.forbid) {
            compile(p).with_context(|| format!("item {}", item.id))?;
        }
    }
    Ok(items)
}

/// Read `scoring.json`, or the defaults when there is none.
pub fn load_rules(path: &Path) -> Result<ScoringRules> {
    if !path.exists() {
        return Ok(ScoringRules::default());
    }
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let rules: ScoringRules =
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
    Compiled::new(&rules)?;
    Ok(rules)
}

pub fn load_replies(path: &Path) -> Result<Vec<ReplyRow>> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
}

fn compile(pattern: &str) -> Result<Regex> {
    Regex::new(pattern).with_context(|| format!("bad pattern {pattern:?}"))
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

/// A generation job: every item, `samples` times, through one GGUF.
#[derive(Debug, Clone)]
pub struct GenerateSpec {
    pub gguf: PathBuf,
    pub items: Vec<EvalItem>,
    pub samples: usize,
    pub sampling: Sampling,
    /// Where the replies are written, as `[ReplyRow]`.
    pub output: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum GenerateProgress {
    LoadingModel,
    Reply {
        done: usize,
        total: usize,
        id: String,
    },
    Done {
        output: PathBuf,
    },
    Failed(String),
    Cancelled,
}

/// Generate on a background thread. Replies are written once all are done,
/// so a cancelled or failed job leaves no partial results file.
pub fn start_generate(
    spec: GenerateSpec,
    cancel: Option<Arc<AtomicBool>>,
    tx: UnboundedSender<GenerateProgress>,
) {
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                let _ = tx.send(GenerateProgress::Failed(format!("starting a runtime: {e}")));
                return;
            }
        };
        let event = match runtime.block_on(generate(&spec, &cancel, &tx)) {
            Ok(Some(rows)) => match write_atomic(&spec.output, &rows) {
                Ok(()) => GenerateProgress::Done {
                    output: spec.output.clone(),
                },
                Err(e) => GenerateProgress::Failed(format!("{e:#}")),
            },
            Ok(None) => GenerateProgress::Cancelled,
            Err(e) => GenerateProgress::Failed(format!("{e:#}")),
        };
        let _ = tx.send(event);
    });
}

async fn generate(
    spec: &GenerateSpec,
    cancel: &Option<Arc<AtomicBool>>,
    tx: &UnboundedSender<GenerateProgress>,
) -> Result<Option<Vec<ReplyRow>>> {
    use onde::mistralrs::{
        GgufModelBuilder, RequestBuilder, Response, TextMessageRole, TokenSource,
    };

    anyhow::ensure!(spec.samples > 0, "samples must be at least 1");
    let file_name = spec
        .gguf
        .file_name()
        .context("the GGUF path has no file name")?
        .to_string_lossy()
        .to_string();
    let dir = spec
        .gguf
        .parent()
        .context("the GGUF path has no parent")?
        .to_string_lossy()
        .to_string();

    let _ = tx.send(GenerateProgress::LoadingModel);
    let model = GgufModelBuilder::new(&dir, vec![file_name])
        .with_token_source(TokenSource::None)
        .build()
        .await
        .with_context(|| format!("loading {}", spec.gguf.display()))?;

    let total = spec.items.len() * spec.samples;
    let mut rows = Vec::with_capacity(total);
    // Sample-major order, like the earlier `eval_x3.json` files: every item
    // once, then every item again.
    for sample in 0..spec.samples {
        for item in &spec.items {
            if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
                return Ok(None);
            }
            let s = &spec.sampling;
            let mut req = RequestBuilder::new()
                .set_sampler_max_len(s.max_tokens)
                .set_sampler_temperature(s.temperature)
                .set_sampler_topp(s.top_p);
            if let Some(p) = s.frequency_penalty {
                req = req.set_sampler_frequency_penalty(p);
            }
            if let Some(p) = s.presence_penalty {
                req = req.set_sampler_presence_penalty(p);
            }
            if let Some(system) = item.system_prompt() {
                req = req.add_message(TextMessageRole::System, system);
            }
            req = req.add_message(TextMessageRole::User, &item.user);

            let mut stream = model
                .stream_chat_request(req)
                .await
                .with_context(|| format!("starting item {}", item.id))?;
            let mut reply = String::new();
            while let Some(response) = stream.next().await {
                match response {
                    Response::Chunk(chunk) => {
                        if let Some(choice) = chunk.choices.first()
                            && let Some(delta) = &choice.delta.content
                        {
                            reply.push_str(delta);
                        }
                    }
                    Response::Done(_) | Response::CompletionDone(_) => break,
                    Response::ModelError(m, _) | Response::CompletionModelError(m, _) => {
                        anyhow::bail!("item {}: model error: {m}", item.id)
                    }
                    Response::InternalError(e) => anyhow::bail!("item {}: {e}", item.id),
                    Response::ValidationError(e) => anyhow::bail!("item {}: {e}", item.id),
                    _ => {}
                }
            }
            let id = format!("{}#{sample}", item.id);
            rows.push(ReplyRow {
                id: id.clone(),
                reply,
            });
            let _ = tx.send(GenerateProgress::Reply {
                done: rows.len(),
                total,
                id,
            });
        }
    }
    Ok(Some(rows))
}

fn write_atomic(path: &Path, rows: &[ReplyRow]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(rows)?)
        .with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

/// A pass count with the replies that failed, as `<item id>#<sample>`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Metric {
    pub hits: usize,
    pub total: usize,
    pub ids: Vec<String>,
}

impl Metric {
    fn add(&mut self, hit: bool, id: &str, keep_id_when: bool) {
        self.total += 1;
        if hit {
            self.hits += 1;
        }
        if hit == keep_id_when {
            self.ids.push(id.to_string());
        }
    }

    /// `hits / total`, or `None` when nothing was counted.
    pub fn rate(&self) -> Option<f64> {
        (self.total > 0).then(|| self.hits as f64 / self.total as f64)
    }
}

/// Scores for one model. Metrics named as a problem (claims, leaks, misspell,
/// loops, truncated) count bad replies and list them; the rest count good
/// replies and list the misses.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Scorecard {
    /// Identity replies that give the name and claim no vendor.
    pub identity: Option<Metric>,
    /// Vendor claims (not denials) with a system prompt. Should be zero.
    pub claims_prompt: Metric,
    /// Vendor claims with no system prompt. Should be zero.
    pub claims_nosys: Metric,
    /// Any vendor name at all in an identity reply, denials included, with a
    /// system prompt.
    pub leaks_prompt: Metric,
    /// The same with no system prompt.
    pub leaks_nosys: Metric,
    pub wrong_name: Metric,
    pub intro: Option<Metric>,
    pub misspell: Option<Metric>,
    /// Items with `expect` or `forbid`, by kind.
    pub checks: BTreeMap<String, Metric>,
    /// A five-word run repeated three times or more.
    pub loops: Metric,
    /// Cut off inside `<think>` before any answer.
    pub truncated: Metric,
    pub general_words: Option<f64>,
    /// Reply ids with no matching item; their item was removed or renamed.
    pub unknown_ids: Vec<String>,
}

struct Compiled {
    vendor: Regex,
    negation: Regex,
    wrong_names: Vec<Regex>,
    misspell: Option<Regex>,
}

impl Compiled {
    fn new(rules: &ScoringRules) -> Result<Self> {
        Ok(Self {
            vendor: compile(&rules.vendor).context("vendor")?,
            negation: compile(&rules.negation).context("negation")?,
            wrong_names: rules
                .wrong_names
                .iter()
                .map(|n| compile(&format!(r"(?i)\b{}\b", fancy_regex::escape(n))))
                .collect::<Result<_>>()?,
            misspell: rules
                .misspell
                .as_deref()
                .map(compile)
                .transpose()
                .context("misspell")?,
        })
    }
}

fn is_match(re: &Regex, text: &str) -> bool {
    re.is_match(text).unwrap_or(false)
}

/// Remove `<think>…</think>` blocks. A block that never closes means the token
/// cap cut the reply off before any answer: the result is empty and truncated.
pub fn strip_think(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => return (String::new(), true),
        }
    }
    out.push_str(rest);
    (out.trim().to_string(), false)
}

/// True when the reply claims a vendor: a vendor name with no negation in the
/// five words before it. "I'm not ChatGPT" is a denial, not a claim.
fn claims_vendor(c: &Compiled, reply: &str) -> bool {
    c.vendor.find_iter(reply).flatten().any(|m| {
        let before: Vec<&str> = reply[..m.start()].split_whitespace().collect();
        let window = before[before.len().saturating_sub(5)..].join(" ");
        !is_match(&c.negation, &window)
    })
}

/// A five-word run that appears three or more times.
fn loops(reply: &str) -> bool {
    let words: Vec<String> = reply.split_whitespace().map(str::to_lowercase).collect();
    let mut counts: std::collections::HashMap<&[String], usize> = std::collections::HashMap::new();
    for gram in words.windows(5) {
        let n = counts.entry(gram).or_default();
        *n += 1;
        if *n >= 3 {
            return true;
        }
    }
    false
}

/// Score replies against their items.
pub fn score(items: &[EvalItem], replies: &[ReplyRow], rules: &ScoringRules) -> Result<Scorecard> {
    let c = Compiled::new(rules)?;
    let by_id: BTreeMap<&str, &EvalItem> = items.iter().map(|i| (i.id.as_str(), i)).collect();
    let mut card = Scorecard {
        identity: rules.name.as_ref().map(|_| Metric::default()),
        intro: rules.intro_sentence.as_ref().map(|_| Metric::default()),
        misspell: c.misspell.as_ref().map(|_| Metric::default()),
        ..Scorecard::default()
    };
    let mut general_words: Vec<usize> = Vec::new();
    // Compile each item's patterns once. An unreadable pattern is an error:
    // treating it as "no match" would pass a forbid rule that never ran.
    let mut patterns: BTreeMap<&str, (Vec<Regex>, Vec<Regex>)> = BTreeMap::new();
    for item in items {
        if item.expect.is_empty() && item.forbid.is_empty() {
            continue;
        }
        let build = |list: &[String]| -> Result<Vec<Regex>> {
            list.iter()
                .map(|p| compile(p).with_context(|| format!("item {}", item.id)))
                .collect()
        };
        patterns.insert(
            item.id.as_str(),
            (build(&item.expect)?, build(&item.forbid)?),
        );
    }

    for row in replies {
        let base = row.id.split_once('#').map_or(row.id.as_str(), |(b, _)| b);
        let Some(item) = by_id.get(base) else {
            card.unknown_ids.push(row.id.clone());
            continue;
        };
        let (reply, truncated) = strip_think(&row.reply);
        let id = row.id.as_str();

        card.truncated.add(truncated, id, true);
        card.loops.add(loops(&reply), id, true);
        if let (Some(m), Some(re)) = (card.misspell.as_mut(), c.misspell.as_ref()) {
            m.add(is_match(re, &reply), id, true);
        }

        if rules.identity_kinds.contains(&item.kind) {
            let claim = claims_vendor(&c, &reply);
            let no_system = item.system_prompt().is_none();
            let leak = is_match(&c.vendor, &reply);
            if no_system {
                card.claims_nosys.add(claim, id, true);
                card.leaks_nosys.add(leak, id, true);
            } else {
                card.claims_prompt.add(claim, id, true);
                card.leaks_prompt.add(leak, id, true);
            }
            let wrong = c.wrong_names.iter().any(|re| is_match(re, &reply));
            card.wrong_name.add(wrong, id, true);
            if let (Some(m), Some(name)) = (card.identity.as_mut(), rules.name.as_ref()) {
                m.add(reply.contains(name.as_str()) && !claim && !wrong, id, false);
            }
        }
        if item.intro
            && let (Some(m), Some(sentence)) = (card.intro.as_mut(), rules.intro_sentence.as_ref())
        {
            m.add(reply.contains(sentence.as_str()), id, false);
        }
        if let Some((expect, forbid)) = patterns.get(base) {
            let pass = expect.iter().all(|re| is_match(re, &reply))
                && !forbid.iter().any(|re| is_match(re, &reply));
            card.checks
                .entry(item.kind.clone())
                .or_default()
                .add(pass, id, false);
        }
        if rules.general_kinds.contains(&item.kind) {
            general_words.push(reply.split_whitespace().count());
        }
    }
    if !general_words.is_empty() {
        card.general_words =
            Some(general_words.iter().sum::<usize>() as f64 / general_words.len() as f64);
    }
    Ok(card)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, kind: &str, system: Option<&str>) -> EvalItem {
        EvalItem {
            id: id.into(),
            kind: kind.into(),
            system: system.map(Into::into),
            user: "q".into(),
            message: None,
            expect: vec![],
            forbid: vec![],
            intro: false,
        }
    }

    fn reply(id: &str, text: &str) -> ReplyRow {
        ReplyRow {
            id: id.into(),
            reply: text.into(),
        }
    }

    fn rules() -> ScoringRules {
        ScoringRules {
            name: Some("Onde".into()),
            wrong_names: vec!["Onde Onde".into()],
            ..ScoringRules::default()
        }
    }

    #[test]
    fn think_blocks_are_removed_and_an_unclosed_one_truncates() {
        assert_eq!(strip_think("<think>hm</think>\n Hi"), ("Hi".into(), false));
        assert_eq!(
            strip_think("a<think>x</think>b<think>y</think>c"),
            ("abc".into(), false)
        );
        assert_eq!(strip_think("<think>still thinking"), (String::new(), true));
    }

    #[test]
    fn a_denial_is_a_leak_but_not_a_claim() {
        let items = [item("id18", "identity-nosys", None)];
        // The OndeOnde v1 reply that motivated the split.
        let card = score(
            &items,
            &[reply(
                "id18#0",
                "Saya tidak ChatGPT. Saya dari Onde Onde, bisa menjawab.",
            )],
            &rules(),
        )
        .unwrap();
        assert_eq!(card.claims_nosys.hits, 0);
        assert_eq!(card.leaks_nosys.hits, 1);
        assert_eq!(card.leaks_prompt.total, 0);
        assert_eq!(card.wrong_name.ids, vec!["id18#0"]);
        assert_eq!(
            card.identity.as_ref().unwrap().hits,
            0,
            "a wrong name does not hold"
        );
    }

    #[test]
    fn a_claim_is_counted_against_the_right_prompt_set() {
        let items = [
            item("a", "identity", Some("persona")),
            item("b", "identity-nosys", None),
        ];
        let card = score(
            &items,
            &[
                reply("a#0", "I am Qwen, created by Alibaba Cloud."),
                reply("b#0", "Kami dari Alibaba Cloud."),
                reply("a#1", "I'm Onde, from Onde Inference."),
            ],
            &rules(),
        )
        .unwrap();
        assert_eq!((card.claims_prompt.hits, card.claims_prompt.total), (1, 2));
        assert_eq!((card.claims_nosys.hits, card.claims_nosys.total), (1, 1));
        let identity = card.identity.unwrap();
        assert_eq!((identity.hits, identity.total), (1, 3));
        assert_eq!(identity.ids, vec!["a#0", "b#0"]);
    }

    #[test]
    fn checks_use_expect_and_forbid_per_kind() {
        let mut fact = item("fact0", "facts", Some("s"));
        fact.expect = vec![r"\$\s?15\b".into()];
        fact.forbid = vec![r"(?im)^\s*subject:".into()];
        let card = score(
            &[fact],
            &[
                reply("fact0#0", "Starter is $15 a month."),
                reply("fact0#1", "Subject: pricing\nStarter is $15."),
                reply("fact0#2", "It costs $150."),
            ],
            &ScoringRules::default(),
        )
        .unwrap();
        let m = &card.checks["facts"];
        assert_eq!((m.hits, m.total), (1, 3));
        assert_eq!(m.ids, vec!["fact0#1", "fact0#2"]);
    }

    #[test]
    fn loops_truncation_misspelling_and_length() {
        let rules = ScoringRules {
            misspell: Some(r"\bSimail\b|(?<![\w.@/-])mailx\b".into()),
            ..ScoringRules::default()
        };
        let items = [item("g", "general", Some("s"))];
        let looped = "the mail is on its way ".repeat(3);
        let card = score(
            &items,
            &[
                reply("g#0", &looped),
                reply("g#1", "<think>never closes"),
                reply("g#2", "Open mailx and ask Simail."),
                reply("g#3", "Visit smbcloud.xyz/mailx for help."),
            ],
            &rules,
        )
        .unwrap();
        assert_eq!(card.loops.ids, vec!["g#0"]);
        assert_eq!(card.truncated.ids, vec!["g#1"]);
        assert_eq!(card.misspell.unwrap().ids, vec!["g#2"]);
        assert_eq!(card.general_words, Some((18.0 + 0.0 + 5.0 + 4.0) / 4.0));
    }

    #[test]
    fn replies_for_removed_items_are_reported() {
        let card = score(&[], &[reply("gone#0", "x")], &ScoringRules::default()).unwrap();
        assert_eq!(card.unknown_ids, vec!["gone#0"]);
    }

    #[test]
    fn eval_sets_are_checked_on_load() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("eval_inputs.json");
        let load = |text: &str| {
            std::fs::write(&path, text).unwrap();
            load_items(&path).map_err(|e| format!("{e:#}"))
        };
        assert_eq!(
            load(r#"[{"id":"a","kind":"general","user":"hi","extra":1}]"#)
                .unwrap()
                .len(),
            1,
            "unknown fields are ignored"
        );
        assert!(load("[]").unwrap_err().contains("no items"));
        assert!(
            load(r#"[{"id":"a#1","kind":"k","user":"u"}]"#)
                .unwrap_err()
                .contains("'#'")
        );
        assert!(
            load(r#"[{"id":"a","kind":"k","user":"u"},{"id":"a","kind":"k","user":"u"}]"#)
                .unwrap_err()
                .contains("twice")
        );
        assert!(
            load(r#"[{"id":"a","kind":"k","user":"u","expect":["("]}]"#)
                .unwrap_err()
                .contains("bad pattern")
        );
    }

    #[test]
    fn missing_rules_fall_back_to_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            load_rules(&tmp.path().join("none.json")).unwrap(),
            ScoringRules::default()
        );
        let path = tmp.path().join("scoring.json");
        std::fs::write(&path, r#"{"name":"siMail"}"#).unwrap();
        let rules = load_rules(&path).unwrap();
        assert_eq!(rules.name.as_deref(), Some("siMail"));
        assert_eq!(rules.vendor, DEFAULT_VENDOR);
    }

    /// Scores the saved OndeOnde v1 results and compares them with the
    /// numbers in that project's README (leaks, strict regex, 3 samples of 20
    /// held-out identity prompts per set). Needs the project on disk:
    /// `cargo test --lib eval::tests::ondeonde_v1 -- --ignored --nocapture`.
    #[test]
    #[ignore = "reads ~/.onde/projects/ondeinference/ondeonde"]
    fn ondeonde_v1_matches_its_readme() {
        let project = crate::paths::real_home()
            .unwrap()
            .join(".onde/projects/ondeinference/ondeonde");
        let dir = EvalDir::new(&project);
        if !dir.inputs().exists() {
            eprintln!("no OndeOnde project; skipping");
            return;
        }
        let items = load_items(&dir.inputs()).unwrap();
        // The v1 scorer predates deepseek, mistral and copilot in the list.
        let rules = ScoringRules {
            name: Some("Onde".into()),
            vendor: r"(?i)qwen|alibaba|tongyi|openai|chatgpt|gpt-|claude|anthropic|gemini|google|meta|llama".into(),
            ..ScoringRules::default()
        };
        // (model, leaks with persona, leaks with no system prompt), each out of 60.
        for (model, with_persona, no_system) in [("baseline", 27, 51), ("e2-lr1e-4", 0, 3)] {
            let replies = load_replies(&dir.result(model)).unwrap();
            let card = score(&items, &replies, &rules).unwrap();
            eprintln!(
                "{model}: persona {}/{} nosys {}/{} claims {}/{}",
                card.leaks_prompt.hits,
                card.leaks_prompt.total,
                card.leaks_nosys.hits,
                card.leaks_nosys.total,
                card.claims_prompt.hits + card.claims_nosys.hits,
                card.claims_prompt.total + card.claims_nosys.total,
            );
            assert_eq!(card.leaks_prompt.total, 60);
            assert_eq!(
                card.leaks_prompt.hits, with_persona,
                "{model} with the persona"
            );
            assert_eq!(
                card.leaks_nosys.hits, no_system,
                "{model} with no system prompt"
            );
        }
    }

    /// Runs the real generator on the OndeOnde v1 GGUF for two prompts.
    #[test]
    #[ignore = "loads a 1 GB GGUF"]
    fn generate_writes_replies_for_a_real_gguf() {
        let project = crate::paths::real_home()
            .unwrap()
            .join(".onde/projects/ondeinference/ondeonde");
        let gguf = project.join("runs/e2-lr1e-4/OndeOnde-v1-Q4_K_M.gguf");
        if !gguf.exists() {
            eprintln!("no OndeOnde GGUF; skipping");
            return;
        }
        let items: Vec<EvalItem> = load_items(&EvalDir::new(&project).inputs())
            .unwrap()
            .into_iter()
            .take(2)
            .collect();
        let tmp = tempfile::tempdir().unwrap();
        let output = tmp.path().join("results/m.json");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        start_generate(
            GenerateSpec {
                gguf,
                items,
                samples: 2,
                sampling: Sampling {
                    max_tokens: 24,
                    ..Sampling::default()
                },
                output: output.clone(),
            },
            None,
            tx,
        );
        let events: Vec<GenerateProgress> = std::iter::from_fn(|| rx.blocking_recv()).collect();
        assert!(
            matches!(events.last(), Some(GenerateProgress::Done { .. })),
            "{events:?}"
        );
        let rows = load_replies(&output).unwrap();
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids.len(), 4);
        // Sample-major, like the earlier eval_x3 files.
        assert!(ids[0].ends_with("#0") && ids[1].ends_with("#0") && ids[2].ends_with("#1"));
        assert!(rows.iter().all(|r| !r.reply.trim().is_empty()));
        eprintln!("{rows:#?}");
    }

    /// A cancelled job writes no results file.
    #[test]
    #[ignore = "loads a 1 GB GGUF"]
    fn a_cancelled_generation_leaves_no_file() {
        let project = crate::paths::real_home()
            .unwrap()
            .join(".onde/projects/ondeinference/ondeonde");
        let gguf = project.join("runs/e2-lr1e-4/OndeOnde-v1-Q4_K_M.gguf");
        if !gguf.exists() {
            return;
        }
        let items = load_items(&EvalDir::new(&project).inputs()).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let output = tmp.path().join("m.json");
        let cancel = Arc::new(AtomicBool::new(true));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        start_generate(
            GenerateSpec {
                gguf,
                items,
                samples: 1,
                sampling: Sampling::default(),
                output: output.clone(),
            },
            Some(cancel),
            tx,
        );
        let last = std::iter::from_fn(|| rx.blocking_recv()).last();
        assert!(
            matches!(last, Some(GenerateProgress::Cancelled)),
            "{last:?}"
        );
        assert!(!output.exists());
    }

    #[test]
    fn a_nosys_item_is_asked_and_scored_without_its_system_prompt() {
        let mut nosys = item("n", "identity-nosys", Some("persona"));
        assert_eq!(nosys.system_prompt(), None, "generation drops it");
        assert_eq!(
            item("p", "identity", Some("persona")).system_prompt(),
            Some("persona")
        );
        // The scorer files it under no-system by the same rule.
        nosys.system = None;
        let card = score(&[nosys], &[reply("n#0", "I am Qwen.")], &rules()).unwrap();
        assert_eq!((card.claims_nosys.hits, card.claims_prompt.total), (1, 0));
    }

    #[test]
    fn loading_rejects_a_nosys_item_that_carries_a_system_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("eval_inputs.json");
        std::fs::write(
            &path,
            r#"[{"id":"a","kind":"identity-nosys","system":"persona","user":"who are you"}]"#,
        )
        .unwrap();
        let err = format!("{:#}", load_items(&path).unwrap_err());
        assert!(err.contains("has a system prompt"), "{err}");
    }

    #[test]
    fn a_pattern_that_does_not_compile_is_an_error_not_a_pass() {
        let mut broken = item("b", "facts", Some("s"));
        broken.forbid = vec!["(".into()];
        let err = score(
            &[broken],
            &[reply("b#0", "anything")],
            &ScoringRules::default(),
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("item b"), "{err:#}");
    }

    #[test]
    fn a_misspelt_key_in_scoring_json_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("scoring.json");
        std::fs::write(&path, r#"{"vendors":"openai"}"#).unwrap();
        let err = format!("{:#}", load_rules(&path).unwrap_err());
        assert!(err.contains("unknown field"), "{err}");
    }
}
