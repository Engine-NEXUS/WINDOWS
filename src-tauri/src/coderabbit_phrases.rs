//! Spoken/typed phrasings for "CodeRabbit review of this PR" and "AI analysis of this PR" (pure; no side effects).
//!
//! Both only fire without an explicit PR number (`coderabbit pr 24 ...` keeps resolving that PR) and never for the
//! plain hub phrases ("open coderabbit", "show coderabbit review", "check coderabbit review"), which keep opening the
//! pull-request list. The input is the parser's normalised lower-case text with "code rabbit" already folded into
//! "coderabbit".

fn words(t: &str) -> Vec<&str> {
    t.split(|c: char| !c.is_alphanumeric() && c != '\'').filter(|w| !w.is_empty()).collect()
}

const REVIEW_WORDS: [&str; 12] = ["review", "reviews", "analyse", "analyze", "analysis", "check", "look", "feedback", "findings", "say", "says", "think"];
const HUB_NOUNS: [&str; 7] = ["hub", "pull requests", "prs", "list", "dashboard", "window", "interface"];

/// "give me coderabbit review", "show me the coderabbit review", "use coderabbit to review this", "coderabbit this PR",
/// "run coderabbit on this", "what does coderabbit say about this PR".
pub fn is_coderabbit_review_request(t: &str) -> bool {
    if !t.contains("coderabbit") || t.chars().any(|c| c.is_ascii_digit()) || HUB_NOUNS.iter().any(|n| t.contains(n)) {
        return false;
    }
    let w = words(t);
    let has = |x: &str| w.contains(&x);
    let review = REVIEW_WORDS.iter().any(|x| has(x));
    let first = w.first().copied().unwrap_or("");
    has("this")
        || has("current")
        || (has("me") && review)
        || (matches!(first, "use" | "run" | "let" | "have" | "ask" | "can" | "could" | "would" | "get") && review)
        || (first == "what" && (has("say") || has("says") || has("think")))
}

const AI_WORDS: [&str; 7] = ["ai", "gemini", "groq", "cerebras", "llm", "chatgpt", "claude"];
const ANALYSE_WORDS: [&str; 9] = ["analyse", "analyze", "analysis", "review", "look", "check", "explain", "summarise", "summarize"];
const TARGET_WORDS: [&str; 7] = ["this", "current", "pr", "error", "errors", "failure", "build"];

/// "have gemini look at this PR", "analyse this error with ai", "ai review this pull request". Needs an AI word, so
/// ordinary "analyse this ..." commands are untouched.
pub fn is_ai_review_request(t: &str) -> bool {
    if t.contains("coderabbit") || t.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let w = words(t);
    let has = |x: &str| w.contains(&x);
    AI_WORDS.iter().any(|x| has(x)) && ANALYSE_WORDS.iter().any(|x| has(x)) && TARGET_WORDS.iter().any(|x| has(x))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent_parser::{parse_deterministic, ParsedIntent};

    #[test]
    fn every_review_phrasing_is_recognised() {
        for p in [
            "give me coderabbit review",
            "give me the coderabbit review",
            "show me the coderabbit review",
            "show me coderabbit review of this pr",
            "use coderabbit to review this",
            "use coderabbit to review",
            "coderabbit this pr",
            "coderabbit this",
            "run coderabbit on this",
            "run coderabbit review",
            "can you coderabbit review this",
            "what does coderabbit say about this pr",
            "what does coderabbit think",
            "let coderabbit review this pull request",
            "analyse this with coderabbit",
            "get me the coderabbit review",
        ] {
            assert!(is_coderabbit_review_request(p), "{p}");
        }
    }

    #[test]
    fn hub_phrases_and_explicit_numbers_are_not_review_requests() {
        for p in [
            "open coderabbit",
            "show coderabbit",
            "coderabbit",
            "coderabbit review",
            "open coderabbit review",
            "show coderabbit review",
            "check coderabbit review",
            "show me the coderabbit pull requests",
            "open the coderabbit hub",
            "show coderabbit review for pr 24",
            "give me coderabbit review of pr 24",
            "open the weather",
            "review this app",
        ] {
            assert!(!is_coderabbit_review_request(p), "{p}");
        }
    }

    #[test]
    fn ai_analysis_needs_an_ai_word_and_leaves_normal_commands_alone() {
        for p in ["have gemini look at this pr", "analyse this error with ai", "ai review this pull request", "ask groq to explain this build failure", "use my llm to check this pr"] {
            assert!(is_ai_review_request(p), "{p}");
        }
        for p in ["analyse this website", "analyse pr 34", "analyse this with coderabbit", "explain this build failure", "open ai", "what is gemini"] {
            assert!(!is_ai_review_request(p), "{p}");
        }
    }

    #[test]
    fn the_full_parser_routes_them_and_keeps_the_old_routes() {
        let route = |p: &str| parse_deterministic(p).unwrap_or_else(|| panic!("no parse for {p:?}")).intent;
        assert!(matches!(route("give me coderabbit review"), ParsedIntent::CodeRabbitReviewThis));
        assert!(matches!(route("use coderabbit to review this"), ParsedIntent::CodeRabbitReviewThis));
        assert!(matches!(route("nexus, show me the code rabbit review"), ParsedIntent::CodeRabbitReviewThis), "wake word and 'code rabbit'");
        assert!(matches!(route("have gemini look at this pr"), ParsedIntent::AiReviewThis));
        assert!(matches!(route("show coderabbit review"), ParsedIntent::OpenCodeRabbitHub));
        assert!(matches!(route("open coderabbit"), ParsedIntent::OpenCodeRabbitHub));
        assert!(matches!(route("coderabbit pr 24"), ParsedIntent::GitHubCommand { .. }));
    }
}
