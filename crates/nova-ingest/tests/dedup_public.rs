use nova_ingest::dedup::{
    align_chapters, classify_pair, fingerprint_book, fingerprint_chapter, normalize_conservative,
    normalize_layout, sha256, source_content_hash, winnow, BookSide, ChapterInput, ChapterMatch,
    ChapterMatchKind, ClassificationThresholds, DedupError, DuplicateRelation, NormalizationLevel,
    Sha256Hash, SourceContentHasher, WinnowingConfig, WinnowingFingerprint,
    CONSERVATIVE_NORMALIZATION_VERSION, LAYOUT_NORMALIZATION_VERSION,
};

#[test]
fn conservative_normalization_stabilizes_text_without_changing_words_or_punctuation() {
    let normalized = normalize_conservative("\u{feff}Cafe\u{301}\r\n \t第二行！\u{200b}  ");

    assert_eq!(normalized, "Café\n第二行！");
    assert_eq!(
        sha256(b"abc").to_hex(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(normalize_conservative("Cafe\u{200b}\u{301}"), "Café");
}

#[test]
fn ordered_source_hash_commits_to_chapter_boundaries_and_order() {
    let original = source_content_hash([(0, "ab"), (1, "c")]);
    assert_ne!(original, source_content_hash([(0, "a"), (1, "bc")]));
    assert_ne!(original, source_content_hash([(1, "c"), (0, "ab")]));
    assert_eq!(original, source_content_hash([(0, "ab"), (1, "c")]));
}

#[test]
fn streaming_source_hash_matches_the_batch_helper() {
    let mut streaming = SourceContentHasher::new();
    streaming.update(0, "ab");
    streaming.update(1, "c");

    assert_eq!(
        streaming.finalize(),
        source_content_hash([(0, "ab"), (1, "c")])
    );
}

#[test]
fn layout_normalization_ignores_formatting_and_compatibility_width() {
    assert_eq!(normalize_layout("Ａ Ｂ\tＣ，\r\n第二章！"), "ABC,第二章!");
    assert_eq!(CONSERVATIVE_NORMALIZATION_VERSION, 1);
    assert_eq!(LAYOUT_NORMALIZATION_VERSION, 1);
}

#[test]
fn sha256_hash_crosses_storage_boundaries_as_hex_or_bytes() {
    let parsed =
        Sha256Hash::from_hex("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
            .expect("known SHA-256 should parse");

    assert_eq!(parsed, sha256(b"abc"));
    assert_eq!(parsed.as_bytes()[0], 0xba);
    assert_eq!(Sha256Hash::from_bytes(*parsed.as_bytes()), parsed);
    assert!(Sha256Hash::from_hex("not-a-sha256").is_err());
}

#[test]
fn book_fingerprint_is_layout_stable_across_chapter_boundaries() {
    let split = fingerprint_book(&[
        ChapterInput {
            chapter_index: 10,
            content: "第１章\n山 川",
        },
        ChapterInput {
            chapter_index: 20,
            content: "第二章\n星海",
        },
    ]);
    let merged = fingerprint_book(&[ChapterInput {
        chapter_index: 7,
        content: "第1章山川第二章星海",
    }]);
    let chapter = fingerprint_chapter(ChapterInput {
        chapter_index: 42,
        content: "第１章\n山 川",
    });

    assert_eq!(split.layout_hash, merged.layout_hash);
    assert_ne!(split.conservative_hash, merged.conservative_hash);
    assert_eq!(split.char_count, 10);
    assert_eq!(split.chapters.len(), 2);
    assert_eq!(chapter.chapter_index, 42);
    assert_eq!(chapter.char_count, 5);
    assert_eq!(chapter.conservative_normalization_version, 1);
    assert_eq!(chapter.layout_normalization_version, 1);
}

#[test]
fn winnowing_is_deterministic_and_retains_normalized_character_positions() {
    let fingerprints = winnow(
        "ａ a\nａa",
        NormalizationLevel::Layout,
        WinnowingConfig {
            gram_size: 2,
            window_size: 2,
        },
    )
    .expect("valid winnowing parameters");

    assert_eq!(
        fingerprints,
        vec![
            WinnowingFingerprint {
                hash: 25_284,
                position: 1,
            },
            WinnowingFingerprint {
                hash: 25_284,
                position: 2,
            },
        ]
    );
}

#[test]
fn winnowing_rejects_zero_parameters_and_ignores_text_without_a_full_window() {
    assert_eq!(
        winnow(
            "正文",
            NormalizationLevel::Conservative,
            WinnowingConfig {
                gram_size: 0,
                window_size: 2,
            },
        ),
        Err(DedupError::InvalidWinnowingConfig)
    );
    assert_eq!(
        winnow(
            "正文",
            NormalizationLevel::Conservative,
            WinnowingConfig {
                gram_size: 2,
                window_size: 0,
            },
        ),
        Err(DedupError::InvalidWinnowingConfig)
    );
    assert!(winnow(
        "短文",
        NormalizationLevel::Conservative,
        WinnowingConfig {
            gram_size: 2,
            window_size: 2,
        },
    )
    .expect("parameters are valid")
    .is_empty());
}

#[test]
fn sparse_alignment_is_one_to_one_and_reports_bidirectional_evidence() {
    let a = make_book(&[(10, "甲 一"), (20, "共同二"), (30, "共同二"), (40, "仅A")]);
    let b = make_book(&[
        (100, "仅B头"),
        (110, "甲一"),
        (120, "共同二"),
        (130, "共同二"),
        (140, "仅B尾"),
    ]);

    let alignment = align_chapters(&a, &b);

    assert_eq!(
        alignment.matches,
        vec![
            ChapterMatch {
                a_index: 10,
                b_index: 110,
                kind: ChapterMatchKind::Layout,
            },
            ChapterMatch {
                a_index: 20,
                b_index: 120,
                kind: ChapterMatchKind::Conservative,
            },
            ChapterMatch {
                a_index: 30,
                b_index: 130,
                kind: ChapterMatchKind::Conservative,
            },
        ]
    );
    assert_eq!(alignment.shared_chapters, 3);
    assert!((alignment.coverage_a.chapters - 0.75).abs() < f64::EPSILON);
    assert!((alignment.coverage_a.chars - 0.8).abs() < f64::EPSILON);
    assert!((alignment.coverage_b.chapters - 0.6).abs() < f64::EPSILON);
    assert!((alignment.coverage_b.chars - (4.0 / 7.0)).abs() < f64::EPSILON);
    assert_eq!(alignment.longest_run, 3);
    assert!((alignment.order_score - 1.0).abs() < f64::EPSILON);
    assert_eq!(alignment.unique_in_a, vec![40]);
    assert_eq!(alignment.unique_in_b, vec![100, 140]);
}

#[test]
fn repeated_chapter_hashes_use_a_bounded_alignment_graph() {
    let inputs: Vec<_> = (0..10_000_u32)
        .map(|chapter_index| ChapterInput {
            chapter_index,
            content: "identical repeated chapter",
        })
        .collect();
    let a = fingerprint_book(&inputs);
    let b = fingerprint_book(&inputs);

    // The naive Cartesian occurrence graph contains 100 million nodes. The
    // bounded rank graph still returns the complete monotonic one-to-one match.
    let alignment = align_chapters(&a, &b);

    assert_eq!(alignment.shared_chapters, 10_000);
    assert_eq!(alignment.longest_run, 10_000);
    assert!((alignment.coverage_a.chapters - 1.0).abs() < f64::EPSILON);
    assert!((alignment.coverage_b.chapters - 1.0).abs() < f64::EPSILON);
    assert!((alignment.order_score - 1.0).abs() < f64::EPSILON);
}

#[test]
fn alignment_exposes_reordering_in_order_score() {
    let a = make_numbered_book(&[0, 1, 2, 3]);
    let b = make_numbered_book(&[0, 2, 1, 3]);

    let alignment = align_chapters(&a, &b);

    assert_eq!(alignment.shared_chapters, 3);
    assert!((alignment.order_score - 0.75).abs() < f64::EPSILON);
    assert_eq!(alignment.longest_run, 1);
    assert_eq!(alignment.unique_in_a.len(), 1);
    assert_eq!(alignment.unique_in_b.len(), 1);
}

#[test]
fn classification_keeps_layout_only_equivalence_below_exact_content() {
    let a = make_book(&[(0, "第１章\n山 川"), (1, "第二章\n星海")]);
    let b = make_book(&[(9, "第1章山川第二章星海")]);
    let alignment = align_chapters(&a, &b);

    let classification = classify_pair(&a, &b, &alignment, &ClassificationThresholds::default());

    assert_ne!(classification.relation, DuplicateRelation::ExactContent);
}

#[test]
fn classification_finds_conservative_exact_content_across_chapter_splits() {
    let a = make_book(&[(0, "山川"), (1, "星海")]);
    let b = make_book(&[(9, "山川星海")]);
    let alignment = align_chapters(&a, &b);

    let classification = classify_pair(&a, &b, &alignment, &ClassificationThresholds::default());

    assert_eq!(classification.relation, DuplicateRelation::ExactContent);
    assert_eq!(classification.contained, None);
}

#[test]
fn compatibility_only_layout_equality_is_not_exact_content() {
    let circled = make_book(&[(0, "序号①代表独立条目")]);
    let digit = make_book(&[(0, "序号1代表独立条目")]);
    let alignment = align_chapters(&circled, &digit);

    let classification = classify_pair(
        &circled,
        &digit,
        &alignment,
        &ClassificationThresholds::default(),
    );

    assert_ne!(classification.relation, DuplicateRelation::ExactContent);
}

#[test]
fn classification_marks_a_ten_chapter_prefix_as_the_contained_version() {
    let short = make_numbered_book(&(0..10).collect::<Vec<_>>());
    let long = make_numbered_book(&(0..12).collect::<Vec<_>>());
    let thresholds = ClassificationThresholds::default();

    let forward = classify_pair(&short, &long, &align_chapters(&short, &long), &thresholds);
    let reverse = classify_pair(&long, &short, &align_chapters(&long, &short), &thresholds);

    assert_eq!(forward.relation, DuplicateRelation::ContainedVersion);
    assert_eq!(forward.contained, Some(BookSide::A));
    assert_eq!(reverse.relation, DuplicateRelation::ContainedVersion);
    assert_eq!(reverse.contained, Some(BookSide::B));
}

#[test]
fn classification_uses_high_coverage_for_books_shorter_than_ten_chapters() {
    let short = make_numbered_book(&[0, 1, 2, 3]);
    let long = make_numbered_book(&[0, 1, 2, 3, 4]);

    let classification = classify_pair(
        &short,
        &long,
        &align_chapters(&short, &long),
        &ClassificationThresholds::default(),
    );

    assert_eq!(classification.relation, DuplicateRelation::ContainedVersion);
    assert_eq!(classification.contained, Some(BookSide::A));
}

#[test]
fn classification_marks_two_substantially_overlapping_editions_as_high_overlap() {
    let mut a_numbers: Vec<u32> = (0..10).collect();
    a_numbers.extend(10..15);
    let mut b_numbers: Vec<u32> = (0..10).collect();
    b_numbers.extend(20..25);
    let a = make_numbered_book(&a_numbers);
    let b = make_numbered_book(&b_numbers);
    let alignment = align_chapters(&a, &b);

    let classification = classify_pair(&a, &b, &alignment, &ClassificationThresholds::default());

    assert_eq!(classification.relation, DuplicateRelation::HighOverlap);
    assert_eq!(classification.contained, None);
}

#[test]
fn ten_shared_chapters_in_two_large_books_are_only_partial_overlap() {
    let mut a_numbers: Vec<u32> = (0..10).collect();
    a_numbers.extend(1_000..1_990);
    let mut b_numbers: Vec<u32> = (0..10).collect();
    b_numbers.extend(2_000..2_990);
    let a = make_numbered_book(&a_numbers);
    let b = make_numbered_book(&b_numbers);
    let alignment = align_chapters(&a, &b);

    let classification = classify_pair(&a, &b, &alignment, &ClassificationThresholds::default());

    assert_eq!(alignment.shared_chapters, 10);
    assert!(alignment.coverage_a.chapters < 0.02);
    assert!(alignment.coverage_b.chapters < 0.02);
    assert_eq!(classification.relation, DuplicateRelation::PartialOverlap);
}

#[test]
fn unrelated_and_empty_books_are_not_duplicates() {
    let a = make_numbered_book(&[0, 1, 2, 3]);
    let b = make_numbered_book(&[100, 101, 102, 103]);
    let empty_a = make_book(&[]);
    let empty_b = make_book(&[]);
    let thresholds = ClassificationThresholds::default();

    assert_eq!(
        classify_pair(&a, &b, &align_chapters(&a, &b), &thresholds).relation,
        DuplicateRelation::NotDuplicate
    );
    assert_eq!(
        classify_pair(
            &empty_a,
            &empty_b,
            &align_chapters(&empty_a, &empty_b),
            &thresholds,
        )
        .relation,
        DuplicateRelation::NotDuplicate
    );
}

fn make_book(chapters: &[(u32, &str)]) -> nova_ingest::dedup::BookFingerprint {
    let inputs: Vec<_> = chapters
        .iter()
        .map(|(chapter_index, content)| ChapterInput {
            chapter_index: *chapter_index,
            content,
        })
        .collect();
    fingerprint_book(&inputs)
}

fn make_numbered_book(chapter_numbers: &[u32]) -> nova_ingest::dedup::BookFingerprint {
    let contents: Vec<_> = chapter_numbers
        .iter()
        .map(|number| format!("第{number}章-独特正文-{number}"))
        .collect();
    let inputs: Vec<_> = chapter_numbers
        .iter()
        .zip(&contents)
        .map(|(chapter_index, content)| ChapterInput {
            chapter_index: *chapter_index,
            content,
        })
        .collect();
    fingerprint_book(&inputs)
}
