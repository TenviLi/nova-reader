//! Pair-level evaluation for manually labeled duplicate relations.

use std::collections::{BTreeMap, BTreeSet};
use std::io::BufRead;

use nova_core::domain::dedup::DuplicateRelation;
use serde::{Deserialize, Serialize};

pub use crate::error::EvaluationError;

const ALL_RELATIONS: [DuplicateRelation; 7] = [
    DuplicateRelation::ExactFile,
    DuplicateRelation::ExactContent,
    DuplicateRelation::ContainedVersion,
    DuplicateRelation::HighOverlap,
    DuplicateRelation::PartialOverlap,
    DuplicateRelation::SemanticRelation,
    DuplicateRelation::NotDuplicate,
];

/// One human-reviewed pair. Additional JSON fields are intentionally allowed
/// so a corpus can retain family, split, language, format and direction data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationLabel {
    pub pair_id: String,
    pub expected_relation: DuplicateRelation,
}

/// One system prediction joined to a label through `pair_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationPrediction {
    pub pair_id: String,
    pub predicted_relation: DuplicateRelation,
}

/// One-vs-rest metrics for a single relation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RelationMetrics {
    pub support: usize,
    pub true_positive: usize,
    pub false_positive: usize,
    pub false_negative: usize,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
}

/// Complete deterministic report for one labeled prediction set.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RelationEvaluationReport {
    pub pairs: usize,
    pub correct: usize,
    pub accuracy: f64,
    /// Macro F1 over relations that have at least one expected example.
    pub macro_f1: f64,
    pub per_relation: BTreeMap<String, RelationMetrics>,
}

/// Parse newline-delimited human labels. Blank lines are ignored.
pub fn read_labels_jsonl(reader: impl BufRead) -> Result<Vec<RelationLabel>, EvaluationError> {
    let labels: Vec<RelationLabel> = read_jsonl(reader)?;
    reject_duplicate_ids(
        labels.iter().map(|label| label.pair_id.as_str()),
        |pair_id| EvaluationError::DuplicateLabel { pair_id },
    )?;
    Ok(labels)
}

/// Parse newline-delimited predictions. Blank lines are ignored.
pub fn read_predictions_jsonl(
    reader: impl BufRead,
) -> Result<Vec<RelationPrediction>, EvaluationError> {
    let predictions: Vec<RelationPrediction> = read_jsonl(reader)?;
    reject_duplicate_ids(
        predictions
            .iter()
            .map(|prediction| prediction.pair_id.as_str()),
        |pair_id| EvaluationError::DuplicatePrediction { pair_id },
    )?;
    Ok(predictions)
}

/// Join labels and predictions strictly by pair ID, then compute one-vs-rest
/// precision, recall and F1 for every wire relation.
pub fn evaluate_relations(
    labels: &[RelationLabel],
    predictions: &[RelationPrediction],
) -> Result<RelationEvaluationReport, EvaluationError> {
    if labels.is_empty() {
        return Err(EvaluationError::EmptyLabels);
    }

    let predictions_by_id: BTreeMap<&str, DuplicateRelation> = predictions
        .iter()
        .map(|prediction| (prediction.pair_id.as_str(), prediction.predicted_relation))
        .collect();
    let label_ids: BTreeSet<&str> = labels.iter().map(|label| label.pair_id.as_str()).collect();
    if let Some(pair_id) = predictions_by_id
        .keys()
        .find(|pair_id| !label_ids.contains(**pair_id))
    {
        return Err(EvaluationError::UnexpectedPrediction {
            pair_id: (*pair_id).to_string(),
        });
    }

    let mut joined = Vec::with_capacity(labels.len());
    for label in labels {
        let predicted = predictions_by_id
            .get(label.pair_id.as_str())
            .copied()
            .ok_or_else(|| EvaluationError::MissingPrediction {
                pair_id: label.pair_id.clone(),
            })?;
        joined.push((label.expected_relation, predicted));
    }

    let correct = joined
        .iter()
        .filter(|(expected, predicted)| expected == predicted)
        .count();
    let mut per_relation = BTreeMap::new();
    let mut supported_f1_total = 0.0;
    let mut supported_relations = 0_usize;
    for relation in ALL_RELATIONS {
        let true_positive = joined
            .iter()
            .filter(|(expected, predicted)| *expected == relation && *predicted == relation)
            .count();
        let false_positive = joined
            .iter()
            .filter(|(expected, predicted)| *expected != relation && *predicted == relation)
            .count();
        let false_negative = joined
            .iter()
            .filter(|(expected, predicted)| *expected == relation && *predicted != relation)
            .count();
        let support = true_positive + false_negative;
        let precision = ratio_or_zero(true_positive, true_positive + false_positive);
        let recall = ratio_or_zero(true_positive, support);
        let f1 = if precision + recall == 0.0 {
            0.0
        } else {
            2.0 * precision * recall / (precision + recall)
        };
        if support > 0 {
            supported_f1_total += f1;
            supported_relations += 1;
        }
        per_relation.insert(
            relation.as_str().to_string(),
            RelationMetrics {
                support,
                true_positive,
                false_positive,
                false_negative,
                precision,
                recall,
                f1,
            },
        );
    }

    Ok(RelationEvaluationReport {
        pairs: joined.len(),
        correct,
        accuracy: ratio_or_zero(correct, joined.len()),
        macro_f1: ratio_f64_or_zero(supported_f1_total, supported_relations),
        per_relation,
    })
}

fn read_jsonl<T: for<'de> Deserialize<'de>>(
    reader: impl BufRead,
) -> Result<Vec<T>, EvaluationError> {
    let mut records = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        records.push(serde_json::from_str(&line).map_err(|source| {
            EvaluationError::InvalidJson {
                line: index + 1,
                source,
            }
        })?);
    }
    Ok(records)
}

fn reject_duplicate_ids<'a>(
    pair_ids: impl Iterator<Item = &'a str>,
    error: impl Fn(String) -> EvaluationError,
) -> Result<(), EvaluationError> {
    let mut seen = BTreeSet::new();
    for pair_id in pair_ids {
        if !seen.insert(pair_id) {
            return Err(error(pair_id.to_string()));
        }
    }
    Ok(())
}

#[allow(clippy::cast_precision_loss)]
fn ratio_or_zero(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[allow(clippy::cast_precision_loss)]
fn ratio_f64_or_zero(numerator: f64, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator / denominator as f64
    }
}
