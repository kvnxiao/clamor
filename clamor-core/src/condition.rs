//! Evaluate a `--when` jq filter against the hook payload.
//!
//! The single entry point [`evaluate`] is total and panic-free: it compiles and
//! runs a real jq filter (via the embedded
//! [`jaq_core`]/[`jaq_std`]/[`jaq_json`] engine) against the raw payload bytes
//! and returns a [`Verdict`]. Every failure mode — non-JSON payload, an
//! unparseable or uncompilable filter, a runtime error, or no output at all —
//! collapses to [`Verdict::Unevaluable`] rather than erroring out of the
//! process, so the caller can keep the never-block invariant.
//!
//! The split between [`Verdict::Fail`] and [`Verdict::Unevaluable`] is the
//! point of the module: a clean falsy result is a genuine "condition not met"
//! and gates the cue *silently*, while an inability to decide is surfaced
//! loudly as a fallback error notification by the caller. A broken gate is
//! never silent.
//!
//! jq truthiness is "neither null nor false", so `[]`, `""`, and `0` are all
//! truthy. Emptiness must therefore be tested with `| length == 0`, never a
//! bare path.

use jaq_core::Compiler;
use jaq_core::Ctx;
use jaq_core::ValT;
use jaq_core::Vars;
use jaq_core::data;
use jaq_core::load::Arena;
use jaq_core::load::File;
use jaq_core::load::Loader;
use jaq_core::unwrap_valr;
use jaq_json::Val;
use jaq_json::read;

/// The outcome of evaluating one `--when` filter against the hook payload.
#[derive(Debug)]
pub enum Verdict {
    /// The filter's first output value is truthy (neither `null` nor `false`):
    /// the configured cue should fire.
    Pass,
    /// The filter cleanly evaluated to a falsy first output (`null` / `false`):
    /// the condition was not met, so the cue is gated *silently*.
    Fail,
    /// The filter could not be evaluated at all: a non-JSON payload, a filter
    /// that fails to parse or compile, a runtime error, or no output. Carries a
    /// human-readable reason for the caller to surface under `CLAMOR_DEBUG`;
    /// the library itself never prints. The caller should withhold the
    /// configured cue and raise a fallback error notification so the
    /// breakage is not silent.
    Unevaluable(String),
}

/// Evaluates a jq `filter` against the raw hook `payload` and reports whether
/// the cue should fire.
///
/// Total and panic-free by construction: see the [module docs](self) for how
/// each failure mode maps onto [`Verdict::Unevaluable`] and why the clean-false
/// [`Verdict::Fail`] is kept distinct from it.
///
/// # Arguments
///
/// * `filter` - the jq filter from a single `--when` flag
/// * `payload` - the raw hook payload bytes read from standard input
///
/// # Examples
///
/// ```
/// use clamor_core::condition::{evaluate, Verdict};
///
/// // An empty `background_tasks` array means no work is in flight: Pass.
/// let payload = br#"{"background_tasks": []}"#;
/// assert!(matches!(
///     evaluate(".background_tasks | length == 0", payload),
///     Verdict::Pass
/// ));
///
/// // A non-empty array gates the cue: Fail.
/// let payload = br#"{"background_tasks": [{"status": "running"}]}"#;
/// assert!(matches!(
///     evaluate(".background_tasks | length == 0", payload),
///     Verdict::Fail
/// ));
/// ```
#[must_use = "the verdict decides whether the cue fires; gating depends on it"]
pub fn evaluate(filter: &str, payload: &[u8]) -> Verdict {
    let input = match read::parse_single(payload) {
        Ok(value) => value,
        Err(error) => return Verdict::Unevaluable(format!("payload is not JSON: {error}")),
    };

    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());

    // The compiled filter is owned (no borrow of `arena`), so the arena only has
    // to outlive loading and compilation, both of which happen in this scope.
    let arena = Arena::default();
    // The loader/compiler report errors as a `Vec<(File, Error)>` collection
    // whose `Error` is not `Display`; echo the offending filter rather than dig
    // into the shape — enough to point a gate author at their typo.
    let Ok(modules) = Loader::new(defs).load(
        &arena,
        File {
            code: filter,
            path: (),
        },
    ) else {
        return Verdict::Unevaluable(format!("cannot parse filter: {filter:?}"));
    };
    let Ok(compiled) = Compiler::default().with_funs(funs).compile(modules) else {
        return Verdict::Unevaluable(format!("cannot compile filter: {filter:?}"));
    };

    // Only the first output value decides the verdict.
    let ctx = Ctx::<data::JustLut<Val>>::new(&compiled.lut, Vars::new([]));
    match compiled.id.run((ctx, input)).map(unwrap_valr).next() {
        Some(Ok(value)) if value.as_bool() => Verdict::Pass,
        Some(Ok(_)) => Verdict::Fail,
        Some(Err(error)) => Verdict::Unevaluable(format!("filter runtime error: {error}")),
        None => Verdict::Unevaluable("filter produced no output".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Convenience: the boolean "does this gate pass" plus a flag for whether
    /// it was even evaluable, so table tests read cleanly.
    fn verdict(filter: &str, payload: &[u8]) -> Verdict {
        evaluate(filter, payload)
    }

    #[test]
    fn background_tasks_length_gate() {
        // The motivating gate: empty array passes, non-empty fails, and an
        // absent field is `null | length == 0` -> true -> passes (old clients).
        assert!(matches!(
            verdict(
                ".background_tasks | length == 0",
                br#"{"background_tasks":[]}"#
            ),
            Verdict::Pass
        ));
        assert!(matches!(
            verdict(
                ".background_tasks | length == 0",
                br#"{"background_tasks":[{"id":"1","status":"running"}]}"#
            ),
            Verdict::Fail
        ));
        assert!(
            matches!(
                verdict(".background_tasks | length == 0", br"{}"),
                Verdict::Pass
            ),
            "an absent field is null|length==0 -> true, so old clients fire"
        );
    }

    #[test]
    fn status_aware_running_gate() {
        // The fallback gate if completed-but-failed tasks ever linger: count only
        // the running ones.
        let filter = "[.background_tasks[] | select(.status == \"running\")] | length == 0";
        assert!(matches!(
            verdict(
                filter,
                br#"{"background_tasks":[{"status":"completed"},{"status":"failed"}]}"#
            ),
            Verdict::Pass
        ));
        assert!(matches!(
            verdict(filter, br#"{"background_tasks":[{"status":"running"}]}"#),
            Verdict::Fail
        ));
    }

    #[test]
    fn string_equality() {
        let filter = ".notification_type == \"permission_prompt\"";
        assert!(matches!(
            verdict(filter, br#"{"notification_type":"permission_prompt"}"#),
            Verdict::Pass
        ));
        assert!(matches!(
            verdict(filter, br#"{"notification_type":"idle_prompt"}"#),
            Verdict::Fail
        ));
    }

    #[test]
    fn boolean_combinators() {
        assert!(matches!(
            verdict(".a and .b", br#"{"a":true,"b":true}"#),
            Verdict::Pass
        ));
        assert!(matches!(
            verdict(".a and .b", br#"{"a":true,"b":false}"#),
            Verdict::Fail
        ));
        assert!(matches!(
            verdict(".a or .b", br#"{"a":false,"b":true}"#),
            Verdict::Pass
        ));
        assert!(matches!(
            verdict("(.a | not)", br#"{"a":false}"#),
            Verdict::Pass
        ));
    }

    #[test]
    fn jq_truthiness_gotcha() {
        // jq truthiness is "neither null nor false": empty array/string and 0 are
        // all truthy. A bare path therefore Passes on `[]`, which is the classic
        // gotcha the docs warn against.
        assert!(matches!(
            verdict(".flag", br#"{"flag":true}"#),
            Verdict::Pass
        ));
        assert!(matches!(
            verdict(".flag", br#"{"flag":false}"#),
            Verdict::Fail
        ));
        assert!(
            matches!(verdict(".flag", br#"{"flag":null}"#), Verdict::Fail),
            "null is falsy"
        );
        assert!(
            matches!(verdict(".flag", br#"{"flag":[]}"#), Verdict::Pass),
            "an empty array is truthy in jq -- this is the gotcha"
        );
        assert!(
            matches!(verdict(".flag", br#"{"flag":0}"#), Verdict::Pass),
            "zero is truthy in jq"
        );
    }

    #[test]
    fn unevaluable_non_json_payload() {
        assert!(matches!(
            verdict(".a", b"not json at all"),
            Verdict::Unevaluable(_)
        ));
    }

    #[test]
    fn unevaluable_unparseable_filter() {
        assert!(matches!(
            verdict("this is | not ( valid jq", br"{}"),
            Verdict::Unevaluable(_)
        ));
    }

    #[test]
    fn unevaluable_runtime_error() {
        // Adding a string to a number is a type clash: a clean compile but a
        // runtime error.
        assert!(matches!(
            verdict(".a + \"x\"", br#"{"a":1}"#),
            Verdict::Unevaluable(_)
        ));
    }

    #[test]
    fn unevaluable_no_output() {
        // `empty` produces no values, so there is no first output to judge.
        assert!(matches!(verdict("empty", br"{}"), Verdict::Unevaluable(_)));
    }

    #[test]
    fn never_panics_on_adversarial_input() {
        // The whole contract: feed garbage filters and payloads; every call must
        // return a Verdict rather than panic or error out of the process.
        let payloads: &[&[u8]] = &[b"", b"{", b"[1,2,3]", b"\"a string\"", b"null", b"\xff\xfe"];
        let filters = [".", "..", ".a.b.c", "1/0", "error", "{", "length", ".[]"];
        for payload in payloads {
            for filter in filters {
                // The contract is simply that it always returns one of the three
                // verdicts rather than panicking or erroring out of the process.
                assert!(matches!(
                    evaluate(filter, payload),
                    Verdict::Pass | Verdict::Fail | Verdict::Unevaluable(_)
                ));
            }
        }
    }
}
