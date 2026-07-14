// Domain types mirroring Rust nova-core models

export type Id = string; // UUIDv7

export type BookFormat = 'txt' | 'epub' | 'pdf' | 'doc' | 'docx' | 'md' | 'html';
export type BookStatus = 'pending' | 'processing' | 'ready' | 'duplicate' | 'failed' | 'archived';
export type ReadingStatus = 'unread' | 'reading' | 'completed' | 'on_hold' | 'dropped';
export type Language = 'zh' | 'en' | 'ja' | 'ko' | 'unknown';
export type UserRole = 'admin' | 'reader' | 'guest';

export interface Book {
	id: Id;
	library_id: Id;
	library_name?: string;
	title: string;
	author: string | null;
	description: string | null;
	cover_path: string | null;
	file_path: string;
	file_hash: string;
	format: BookFormat;
	status: BookStatus;
	reading_status: ReadingStatus;
	language: Language;
	word_count: number | null;
	chapter_count: number;
	progress: number; // 0.0 - 1.0
	rating: number | null;
	tags: string[];
	genres?: string[];
	series: string | null;
	series_index: number | null;
	file_size_bytes?: number;
	series_name?: string;
	current_chapter_title?: string;
	volume_label?: string;
	has_entities?: boolean;
	entity_count?: number;
	metadata: BookMetadata;
	created_at: string;
	updated_at: string;
}

export interface BookMetadata {
	publisher?: string;
	publish_date?: string;
	isbn?: string;
	source_url?: string;
	custom: Record<string, string>;
}

export type DuplicateScanStatus = 'queued' | 'running' | 'completed' | 'failed';
export type DuplicateScanPhase =
	| 'recovering'
	| 'retrying'
	| 'failed'
	| 'fingerprinting'
	| 'candidate_generation'
	| 'verifying'
	| 'completed';

export interface DuplicateScan {
	id: Id;
	library_id: Id | null;
	task_id: Id | null;
	include_semantic: boolean;
	algorithm_version: number;
	status: DuplicateScanStatus;
	progress: number;
	progress_message: DuplicateScanPhase | null;
	books_total: number;
	books_processed: number;
	chapters_processed: number;
	candidates_found: number;
	pairs_found: number;
	exact_pairs: number;
	contained_pairs: number;
	semantic_pairs: number;
	error_message: string | null;
	started_at: string | null;
	completed_at: string | null;
	created_at: string;
}

export interface StartDuplicateScanInput {
	library_id?: Id;
	include_semantic?: boolean;
}

export type ExactFileDiscoverySource = 'upload' | 'library_scan';

export interface ExactFileDiscovery {
	id: Id;
	library_id: Id | null;
	matched_book_id: Id;
	matched_book_title: string;
	matched_book_author: string | null;
	matched_book_format: string;
	source_kind: ExactFileDiscoverySource;
	source_path: string;
	file_hash: string;
	file_size_bytes: number;
	first_seen_at: string;
	last_seen_at: string;
	seen_count: number;
}

export interface ExactFileDiscoveryPage {
	items: ExactFileDiscovery[];
	total: number;
	limit: number;
	offset: number;
}

export type DuplicateRelation =
	| 'exact_file'
	| 'exact_content'
	| 'contained_version'
	| 'high_overlap'
	| 'partial_overlap'
	| 'semantic_relation';
export type DuplicateCandidateKind = 'content' | 'semantic';
export type DuplicatePairStatus = 'pending' | 'confirmed' | 'dismissed' | 'deferred';
export type DuplicateResolutionAction = 'keep_a' | 'keep_b' | 'same_work' | 'dismiss' | 'defer';

export interface DuplicateBookSummary {
	id: Id;
	title: string;
	author: string | null;
	format: string;
	file_size: number;
	word_count: number;
	chapter_count: number;
	cover_path: string | null;
}

export interface DuplicateSemanticChapterMatch {
	chapter_a_index: number;
	chapter_b_index: number;
	score: number;
}

export interface DuplicateSemanticEvidence {
	score: number;
	independent_chunk_matches: number;
	independent_chapter_matches: number;
	ordered_chapter_matches: DuplicateSemanticChapterMatch[];
	matched_chapters_a: number;
	matched_chapters_b: number;
	order_score: number;
	sampled_chapters_a: number;
	sampled_chapters_b: number;
	sample_coverage_a: number | null;
	sample_coverage_b: number | null;
	book_chapters_a: number;
	book_chapters_b: number;
	observed_book_coverage_a: number;
	observed_book_coverage_b: number;
}

export interface DuplicatePrimaryVersionEvidence {
	content_chars: number;
	unique_informative_chars: number;
	total_chapters: number;
	informative_chapters: number;
	unique_informative_chapters: number;
	repeated_informative_chapters: number;
	informative_chapter_ratio: number;
	unique_informative_ratio: number;
	word_count: number;
	metadata_quality: number;
	format_quality: number;
	file_size_bytes: number;
	text_integrity_score: number;
}

export interface DuplicatePrimaryRecommendationEvidence {
	recommended_book_id: Id | null;
	unique_informative_content_dominates: boolean;
	reader_assets_considered: boolean;
	book_a: DuplicatePrimaryVersionEvidence;
	book_b: DuplicatePrimaryVersionEvidence;
}

export interface DuplicateAlignmentGroupEvidence {
	id: number;
	mapping_shape: 'one_to_one' | 'one_to_many' | 'many_to_one' | 'many_to_many';
	chapters_a: number[];
	chapters_b: number[];
	matched_characters: number;
	segment_count: number;
	source_verified: boolean;
}

export interface DuplicatePairEvidence {
	schema_version: 'v2';
	exact_file: boolean;
	exact_content: boolean;
	shared_chapter_hashes: number;
	shared_passage_hashes: number;
	semantic_hits: number;
	semantic: DuplicateSemanticEvidence | null;
	primary_recommendation: DuplicatePrimaryRecommendationEvidence | null;
	equivalent_chapters: number;
	matched_chapters_a: number;
	matched_chapters_b: number;
	shared_characters: number;
	unique_characters_a: number;
	unique_characters_b: number;
	alignment_schema_version: number;
	chapter_boundary_groups: DuplicateAlignmentGroupEvidence[];
	unique_chapters_a: number[];
	unique_chapters_b: number[];
	book_a_layout_hash: string;
	book_b_layout_hash: string;
	algorithm_version: number;
}

export interface DuplicatePair {
	id: Id;
	relation: DuplicateRelation;
	status: DuplicatePairStatus;
	confidence: number;
	shared_chapters: number;
	coverage_a: number;
	coverage_b: number;
	character_coverage_a: number;
	character_coverage_b: number;
	longest_contiguous_run: number;
	order_score: number;
	contained_book_id: Id | null;
	recommended_primary_id: Id | null;
	semantic_score: number | null;
	evidence: DuplicatePairEvidence;
	created_at: string;
	updated_at: string;
	book_a: DuplicateBookSummary;
	book_b: DuplicateBookSummary;
}

export interface DuplicatePairPage {
	items: DuplicatePair[];
	total: number;
	limit: number;
	offset: number;
}

export interface DuplicateChapterMatch {
	id: Id;
	match_type: string;
	similarity: number;
	shared_fingerprints: number;
	alignment_group: number | null;
	segment_ordinal: number | null;
	chapter_a_start: number | null;
	chapter_a_end: number | null;
	chapter_b_start: number | null;
	chapter_b_end: number | null;
	matched_chars: number;
	chapter_a_id: Id | null;
	chapter_a_index: number | null;
	chapter_a_title: string | null;
	chapter_b_id: Id | null;
	chapter_b_index: number | null;
	chapter_b_title: string | null;
}

export interface DuplicatePairDetail extends DuplicatePair {
	chapter_matches: DuplicateChapterMatch[];
	chapter_matches_total: number;
	chapter_matches_limit: number;
	chapter_matches_offset: number;
	matched_indices_a: number[];
	matched_indices_b: number[];
}

export interface DuplicatePairFilters {
	candidate_kind?: DuplicateCandidateKind;
	relation?: DuplicateRelation;
	status?: DuplicatePairStatus;
	library_id?: Id;
	limit?: number;
	offset?: number;
}

export interface DuplicateResolutionResult {
	pair_id: Id;
	action: DuplicateResolutionAction;
	status: DuplicatePairStatus;
	work_id?: Id;
	primary_book_id?: Id;
	secondary_book_id?: Id;
	secondary_status?: 'duplicate';
	source_file_deleted?: boolean;
	reader_artifacts_mapped?: number;
	library_links_copied?: number;
	index_cleanup_task_id?: Id;
	archived?: boolean;
}

export interface DuplicateDiffChange {
	tag: 'equal' | 'insert' | 'delete';
	value: string;
}

export interface DuplicateMatchDiff {
	pair_id: Id;
	match_id: Id;
	chapter_a: { id: Id; title: string | null; character_count: number };
	chapter_b: { id: Id; title: string | null; character_count: number };
	changes: DuplicateDiffChange[];
	ratio: number;
	truncated: boolean;
}

export interface Chapter {
	id: Id;
	book_id: Id;
	index: number;
	title: string;
	content: string | null; // Only loaded on demand
	word_count: number;
	start_offset: number;
	end_offset: number;
}

export interface Library {
	id: Id;
	name: string;
	root_path: string;
	description: string | null;
	auto_scan: boolean;
	scan_interval_secs: number;
	include_extensions: string[];
	exclude_patterns: string[];
	series_count: number;
	book_count: number;
	total_size_bytes: number;
	last_scan_at: string | null;
	last_scan_duration_ms: number | null;
	scan_status: string;
	created_at: string;
	updated_at: string;
}

export interface ReadingProgress {
	book_id: Id;
	chapter_index: number;
	current_chapter?: number;
	scroll_position: number;
	progress: number;
	reading_time_secs?: number;
	last_read_at?: string | null;
	updated_at: string;
}

export interface Annotation {
	id: Id;
	book_id: Id;
	chapter_index: number;
	start_offset: number;
	end_offset: number;
	selected_text: string;
	note: string | null;
	color: string;
	created_at: string;
}

export interface Bookmark {
	id: Id;
	book_id: Id;
	chapter_index: number | null;
	position: number | null;
	title: string | null;
	created_at: string;
}

export type EntityType = 'character' | 'person' | 'location' | 'organization' | 'item' | 'skill' | 'event' | 'concept';

export interface Entity {
	id: Id;
	name: string;
	type: EntityType;
	entity_type?: string;
	description: string;
	aliases: string[];
	first_appearance_book: Id | null;
	first_appearance_chapter: number | null;
	mention_count: number;
	book_count: number;
	relationships: EntityRelationship[];
	tags: string[];
	created_at: string;
	updated_at: string;
}

export interface EntityRelationship {
	id: Id;
	source_entity_id: Id;
	target_entity_id: Id;
	relationship_type: string;
	description: string | null;
	weight: number;
	source_name?: string;
	target_name?: string;
}

export interface EntityMention {
	entity_id: Id;
	book_id: Id;
	chapter_index: number;
	start_offset: number;
	end_offset: number;
}

export interface GlossaryEntry {
	id: Id;
	source_term: string;
	target_term: string;
	source_language: Language;
	target_language: Language;
	category: 'character' | 'location' | 'technique' | 'item' | 'title' | 'other';
	context: string | null;
	book_id: Id | null;
	created_at: string;
}

export interface AppliedGlossaryMatch {
	source_term: string;
	target_term: string;
	category: string;
}

export interface TranslateTextResult {
	translated_text: string;
	glossary_applied: AppliedGlossaryMatch[];
	confidence: number;
}

export interface CreateGlossaryEntryInput {
	term: string;
	definition?: string | null;
	source_language?: string | null;
	target_language?: string | null;
	book_id?: Id | null;
}

export interface AiSummaryResult {
	summary: string;
	key_points: string[];
}

export interface AiExtractedEntity {
	name: string;
	entity_type: string;
	description: string;
	aliases: string[];
}

export interface AiExtractedRelationship {
	source: string;
	target: string;
	relationship_type: string;
	description: string;
}

export interface AiExtractEntitiesResult {
	entities: AiExtractedEntity[];
	relationships: AiExtractedRelationship[];
}

export interface AiSuggestedTagsResult {
	genres: string[];
	tags: string[];
	themes: string[];
}

export interface GlossaryExtractionTerm {
	source: string;
	target: string;
	category: string;
	context: string;
}

export interface GlossaryExtractionResult {
	terms: GlossaryExtractionTerm[];
}

export interface PlotHoleIssue {
	severity: string;
	type: string;
	description: string;
	chapters: number[];
	entities: string[];
	suggestion: string;
}

export interface PlotHoleReport {
	issues: PlotHoleIssue[];
	consistency_score: number;
	summary: string;
}

export interface ChapterTitleSuggestion {
	title: string;
	style: string;
}

export interface ChapterTitleResult {
	titles: ChapterTitleSuggestion[];
	recommended: number;
}

export interface CleanupForumTextResult {
	cleaned_text: string;
	removed_count: number;
}

export interface CommunitySummaryResult {
	status: string;
	book_id?: Id;
	total_communities?: number;
	summarized: number;
	errors: string[];
}

export interface Translation {
	id: Id;
	book_id: Id;
	chapter_index: number;
	source_language: Language;
	target_language: Language;
	source_text: string;
	translated_text: string;
	glossary_applied: boolean;
	model_used: string;
	created_at: string;
}

export type TaskKind =
	| 'parse_file'
	| 'generate_embeddings'
	| 'extract_entities'
	| 'deduplicate'
	| 'translate'
	| 'clean_content'
	| 'library_scan'
	| 'generate_metadata'
	| 'build_graph_summary'
	| 'index_meilisearch'
	| 'sync_neo4j'
	| 'compute_book_embedding'
	| 'detect_communities'
	| 'deep_analysis'
	| 'sentiment_arc'
	| 'track_foreshadowing'
	| 'semantic_tagging'
	| 'assign_ontology'
	| 'reindex_library'
	| 'cleanup_orphan_covers'
	| 'recompute_file_hashes';

export type TaskStatus = 'queued' | 'running' | 'completed' | 'failed' | 'cancelled' | 'dead_letter';
export type TaskPriority = 1 | 2 | 3 | 4 | 5; // 1 = highest

export interface Task {
	id: Id;
	kind: TaskKind;
	status: TaskStatus;
	priority: TaskPriority;
	progress: number;
	error: string | null;
	metadata: Record<string, string>;
	retry_count: number;
	max_retries: number;
	created_at: string;
	started_at: string | null;
	completed_at: string | null;
}

export type SearchMode = 'keyword' | 'semantic' | 'hybrid' | 'graph';

export interface SearchQuery {
	query: string;
	mode?: SearchMode;
	book_ids?: Id[];
	page?: number;
	limit?: number;
	offset?: number;
}

export interface SearchResult {
	id: Id;
	book_id: Id;
	book_title: string;
	chapter_index: number;
	chapter_title: string;
	chunk_index?: number;
	content: string;
	content_snippet?: string;
	highlighted?: string;
	score: number;
	rerank_score?: number;
	rerank_explanation?: string;
	rerank_matched_terms?: string[];
	fusion_score?: number;
	keyword_score?: number;
	semantic_score?: number;
	match_sources?: string[];
	breadcrumb?: string;
	source?: 'keyword' | 'semantic' | 'hybrid' | 'graph' | 'database';
	highlight_ranges: [number, number][];
	entities: string[];
}

export interface CrossBookSearchChunk {
	id?: Id;
	chunk_id?: Id;
	chapter_index: number;
	chapter_title?: string | null;
	chunk_index?: number | null;
	content: string;
	highlighted?: string | null;
	score: number;
}

export interface CrossBookSearchGroup {
	book_id: Id;
	book_title: string;
	count: number;
	top_score: number;
	chunks: CrossBookSearchChunk[];
}

export interface CrossBookSearchResponse {
	groups: CrossBookSearchGroup[];
	total: number;
	query?: string;
	timing: { total_ms: number };
}

export interface SmartShelf {
	id: Id;
	name: string;
	description?: string | null;
	filter_criteria: Record<string, unknown>;
	created_at?: string;
}

export interface SmartShelfBook {
	id: Id;
	title: string;
	author?: string | null;
	cover_path?: string | null;
}

export interface Collection {
	id: Id;
	name: string;
	description: string | null;
	book_count: number;
	book_ids?: Id[];
	cover_books: Id[];
	created_at: string;
}

export interface Shelf {
	id: Id;
	name: string;
	is_system: boolean;
	book_count: number;
}

export interface ReadingSession {
	id: Id;
	book_id: Id;
	start_time: string;
	end_time: string | null;
	pages_read: number;
	duration_secs: number;
}

export interface ReadingStats {
	total_reading_time_secs: number;
	books_completed: number;
	total_annotations: number;
	streak_days: number;
	weekly_minutes: number[];
	monthly_books: number[];
}

// API response types
export interface PaginatedResponse<T> {
	data: T[];
	items?: T[];
	total: number;
	page: number;
	per_page: number;
	total_pages: number;
}

export interface ApiError {
	code: string;
	message: string;
	details?: Record<string, unknown>;
}

// Smart Filter types for composite query building
export type FilterOperator = 'eq' | 'neq' | 'gt' | 'gte' | 'lt' | 'lte' | 'contains' | 'not_contains' | 'in' | 'not_in';
export type FilterConjunction = 'and' | 'or';

export interface FilterCondition {
	field: string;
	operator: FilterOperator;
	value: string | number | string[];
}

export interface SmartFilter {
	id: Id;
	name: string;
	conjunction: FilterConjunction;
	conditions: FilterCondition[];
	sort_by?: string;
	sort_dir?: 'asc' | 'desc';
	created_at: string;
}

// ─── Series ─────────────────────────────────────────────────
export type SeriesStatus = 'ongoing' | 'completed' | 'hiatus' | 'unknown';

export interface Series {
	id: Id;
	name: string;
	original_name: string | null;
	description: string | null;
	author?: string | null;
	status: SeriesStatus;
	book_count: number;
	total_word_count: number;
	cover_path: string | null;
	book_covers?: string[];
	folder_path: string | null;
	library_id: Id | null;
	metadata: SeriesMetadataFull | null;
	created_at: string;
	updated_at: string;
}

export interface SeriesMetadataFull {
	summary?: string;
	user_rating?: number;
	genres?: string[];
	tags?: string[];
	[key: string]: unknown;
}

export interface SeriesMetadata {
	description?: string;
	status?: SeriesStatus | string;
	cover_path?: string;
	genres?: string[];
	tags?: string[];
	user_rating?: number;
}

// ─── Person ─────────────────────────────────────────────────
export interface Person {
	id: Id;
	name: string;
	original_name: string | null;
	avatar_path: string | null;
	roles: string[];
	biography: string | null;
	book_count: number;
	total_word_count: number;
	created_at?: string;
}

export interface PersonBook {
	id: Id;
	title: string;
	cover_path: string | null;
	word_count: number;
	role: string;
}

// ─── Settings ───────────────────────────────────────────────
export interface AppSettings {
	theme?: string;
	language?: string;
	reader?: Partial<ReaderSettings>;
	library?: Partial<LibrarySettings>;
	ai?: Partial<AiSettings>;
	[key: string]: unknown;
}

export interface ReaderSettings {
	font_size: number;
	font_family: string;
	line_height: number;
	max_width: number;
	text_indent: boolean;
	justify: boolean;
}

export interface LibrarySettings {
	default_view: string;
	sort_by: string;
	sort_dir: 'asc' | 'desc';
	show_unread_badge: boolean;
}

export interface AiSettings {
	llm_endpoint: string;
	llm_model: string;
	llm_api_key: string;
	embedding_endpoint: string;
	embedding_model: string;
	reranker_endpoint: string;
	reranker_model: string;
	reranker_enabled: boolean;
	qdrant_url: string;
	features: Record<string, boolean>;
	[key: string]: unknown;
}

// ─── Semantic Tags ──────────────────────────────────────────
export interface SemanticProfile {
	id: Id;
	name: string;
	description: string | null;
	category: string;
	color: string;
	reference_texts: string[];
	match_threshold: number;
	is_warning: boolean;
	has_embedding: boolean;
	created_at: string;
}

export interface TagScore {
	tag_profile_id: string;
	name: string;
	color: string;
	category: string;
	is_warning: boolean;
	concentration: number;
	match_count: number;
	total_chunks: number;
	peak_chapter: number | null;
	peak_score: number | null;
}

export interface TagHeatmapEntry {
	chapter_index: number;
	tag_profile_id: string;
	name: string;
	color: string;
	avg_score: number;
	max_score: number;
	match_count: number;
}

export interface TagMarker {
	id?: Id;
	tag_profile_id: Id;
	profile_id: Id;
	chapter_index: number;
	chunk_index?: number;
	similarity_score: number;
	score: number;
	content_snippet: string;
	snippet: string;
	char_offset?: number | null;
	offset?: number | null;
	created_at?: string;
}

export interface SemanticOverview {
	total_profiles: number;
	total_books_tagged: number;
	categories: Record<string, number>;
}

export interface RadarDataPoint {
	name: string;
	color: string;
	score: number;
}

export interface BookRadarResult {
	axes: RadarDataPoint[];
}

export interface VibeSearchResult {
	book_id: Id;
	book_title: string;
	score: number;
	similarity?: number;
	snippet: string;
	content?: string;
	chapter_index?: number;
}

export interface VibeBookmark {
	id: Id;
	name?: string | null;
	source_text: string;
	source_book_id?: Id | null;
	source_chapter_index?: number | null;
	created_at: string;
}

export type NotificationLevel = 'info' | 'success' | 'warning' | 'error';
export type NotificationCategory = 'system' | 'reading' | 'ai' | 'library' | 'social';

export interface AppNotification {
	id: Id;
	level: NotificationLevel;
	category: NotificationCategory;
	title: string;
	body: string;
	link?: string | null;
	book_id?: string | null;
	metadata?: Record<string, unknown>;
	read: boolean;
	created_at: string;
}

export interface LibraryUserPermission {
	user_id: Id;
	can_read: boolean;
	can_write: boolean;
	can_manage: boolean;
}

export interface LibraryGroupPermission {
	group_id: Id;
	can_read: boolean;
	can_write: boolean;
	can_manage: boolean;
}

export interface LibraryPermissionsResponse {
	permissions: LibraryUserPermission[];
	group_permissions: LibraryGroupPermission[];
}

export interface PermissionTemplate {
	id: Id;
	name: string;
	description: string | null;
	can_read: boolean;
	can_write: boolean;
	can_manage: boolean;
	is_system: boolean;
	created_at?: string;
	updated_at?: string;
}

export interface LibraryFeatures {
	enable_ai: boolean;
	enable_translation: boolean;
	enable_graph: boolean;
	allow_guests: boolean;
}
