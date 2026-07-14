use std::io::Cursor;

use nova_ingest::dedup_benchmark::{
    run_synthetic_benchmark, SyntheticBenchmarkConfig, SyntheticBenchmarkError,
};
use nova_ingest::dedup_evaluation::{
    evaluate_relations, read_labels_jsonl, read_predictions_jsonl, EvaluationError,
};

#[test]
fn relation_evaluation_reports_a_known_confusion_matrix() {
    let labels = read_labels_jsonl(Cursor::new(
        br#"{"pair_id":"p1","expected_relation":"exact_content"}
{"pair_id":"p2","expected_relation":"exact_content"}
{"pair_id":"p3","expected_relation":"contained_version"}
{"pair_id":"p4","expected_relation":"not_duplicate"}
"#,
    ))
    .expect("valid labels should parse");
    let predictions = read_predictions_jsonl(Cursor::new(
        br#"{"pair_id":"p1","predicted_relation":"exact_content"}
{"pair_id":"p2","predicted_relation":"contained_version"}
{"pair_id":"p3","predicted_relation":"contained_version"}
{"pair_id":"p4","predicted_relation":"not_duplicate"}
"#,
    ))
    .expect("valid predictions should parse");

    let report = evaluate_relations(&labels, &predictions)
        .expect("one prediction exists for every labeled pair");

    assert_eq!(report.pairs, 4);
    assert!((report.accuracy - 0.75).abs() < f64::EPSILON);

    let exact = &report.per_relation["exact_content"];
    assert_eq!(
        (
            exact.true_positive,
            exact.false_positive,
            exact.false_negative
        ),
        (1, 0, 1)
    );
    assert!((exact.precision - 1.0).abs() < f64::EPSILON);
    assert!((exact.recall - 0.5).abs() < f64::EPSILON);
    assert!((exact.f1 - (2.0 / 3.0)).abs() < 1e-12);

    let contained = &report.per_relation["contained_version"];
    assert_eq!(
        (
            contained.true_positive,
            contained.false_positive,
            contained.false_negative,
        ),
        (1, 1, 0)
    );
    assert!((contained.precision - 0.5).abs() < f64::EPSILON);
    assert!((contained.recall - 1.0).abs() < f64::EPSILON);
    assert!((contained.f1 - (2.0 / 3.0)).abs() < 1e-12);
}

#[test]
fn relation_evaluation_rejects_incomplete_or_ambiguous_inputs() {
    let labels = read_labels_jsonl(Cursor::new(
        br#"{"pair_id":"p1","expected_relation":"exact_content"}
"#,
    ))
    .expect("valid labels should parse");
    let no_predictions = read_predictions_jsonl(Cursor::new(Vec::<u8>::new()))
        .expect("an empty prediction file is syntactically valid");
    assert!(matches!(
        evaluate_relations(&labels, &no_predictions),
        Err(EvaluationError::MissingPrediction { pair_id }) if pair_id == "p1"
    ));

    let duplicate_predictions = read_predictions_jsonl(Cursor::new(
        br#"{"pair_id":"p1","predicted_relation":"exact_content"}
{"pair_id":"p1","predicted_relation":"not_duplicate"}
"#,
    ));
    assert!(matches!(
        duplicate_predictions,
        Err(EvaluationError::DuplicatePrediction { pair_id }) if pair_id == "p1"
    ));
}

#[test]
fn synthetic_capacity_run_has_reproducible_candidates_and_relations() {
    let config = SyntheticBenchmarkConfig {
        books: 41,
        chapters_per_book: 12,
        related_pair_every: 10,
        contained_added_chapters: 3,
        max_hash_document_frequency: 50,
        min_shared_chapters: 2,
        seed: 7,
    };

    let report = run_synthetic_benchmark(&config).expect("the benchmark config is valid");

    assert_eq!(report.books, 41);
    assert_eq!(report.expected_related_pairs, 4);
    assert_eq!(report.candidate_pairs, 4);
    assert_eq!(report.false_candidate_pairs, 0);
    assert!((report.candidate_recall - 1.0).abs() < f64::EPSILON);
    assert_eq!(report.expected_relation_correct, 4);
    assert_eq!(report.verified_relation_counts["exact_content"], 2);
    assert_eq!(report.verified_relation_counts["contained_version"], 1);
    assert_eq!(report.verified_relation_counts["high_overlap"], 1);
    assert_eq!(report.corpus_digest_sha256.len(), 64);
}

#[test]
fn synthetic_capacity_run_rejects_configs_that_cannot_exercise_candidates() {
    let invalid = SyntheticBenchmarkConfig {
        books: 1,
        ..SyntheticBenchmarkConfig::default()
    };

    assert!(matches!(
        run_synthetic_benchmark(&invalid),
        Err(SyntheticBenchmarkError::TooFewBooks)
    ));
}

#[test]
fn documented_jsonl_fixture_is_directly_evaluable() {
    let labels = read_labels_jsonl(Cursor::new(include_str!(
        "../../../docs/evaluation/dedup-labels.example.jsonl"
    )))
    .expect("documented labels should remain valid JSONL");
    let predictions = read_predictions_jsonl(Cursor::new(include_str!(
        "../../../docs/evaluation/dedup-predictions.example.jsonl"
    )))
    .expect("documented predictions should remain valid JSONL");

    let report = evaluate_relations(&labels, &predictions)
        .expect("documented fixtures should join one-to-one");
    assert_eq!(report.pairs, 7);
    assert_eq!(report.correct, 6);
}

#[test]
#[ignore = "manual capacity profile; prefer the release-mode dedup_bench example for timings"]
fn synthetic_capacity_profile_1k() {
    run_manual_capacity_profile(1_000);
}

#[test]
#[ignore = "manual capacity profile; prefer the release-mode dedup_bench example for timings"]
fn synthetic_capacity_profile_10k() {
    run_manual_capacity_profile(10_000);
}

fn run_manual_capacity_profile(books: usize) {
    let report = run_synthetic_benchmark(&SyntheticBenchmarkConfig {
        books,
        ..SyntheticBenchmarkConfig::default()
    })
    .expect("the documented manual profile is valid");
    let rendered = serde_json::to_string_pretty(&report).expect("the report should serialize");
    println!("{rendered}");

    assert_eq!(report.candidate_pairs, report.expected_related_pairs);
    assert_eq!(report.false_candidate_pairs, 0);
    assert!((report.candidate_recall - 1.0).abs() < f64::EPSILON);
    assert_eq!(
        report.expected_relation_correct,
        report.expected_related_pairs
    );
}
