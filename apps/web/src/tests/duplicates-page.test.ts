import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/services/api';
import type { DuplicatePair, DuplicateScan } from '$lib/types/models';
import DuplicatePageHarness from './fixtures/DuplicatePageHarness.svelte';

const pair: DuplicatePair = {
	id: 'pair-1',
	relation: 'contained_version',
	status: 'pending',
	confidence: 0.98,
	shared_chapters: 12,
	coverage_a: 1,
	coverage_b: 0.6,
	character_coverage_a: 0.99,
	character_coverage_b: 0.58,
	longest_contiguous_run: 10,
	order_score: 1,
	contained_book_id: 'book-a',
	recommended_primary_id: 'book-b',
	semantic_score: null,
	evidence: {
		schema_version: 'v2',
		exact_file: false,
		exact_content: false,
		shared_chapter_hashes: 10,
		shared_passage_hashes: 20,
		semantic_hits: 0,
		semantic: null,
		alignment_schema_version: 2,
		equivalent_chapters: 10,
		matched_chapters_a: 10,
		matched_chapters_b: 1,
		shared_characters: 100_000,
		unique_characters_a: 10_000,
		unique_characters_b: 110_000,
		chapter_boundary_groups: [{
			id: 0,
			mapping_shape: 'many_to_one',
			chapters_a: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
			chapters_b: [0],
			matched_characters: 100_000,
			segment_count: 10,
			source_verified: true,
		}],
		primary_recommendation: {
			recommended_book_id: 'book-b',
			unique_informative_content_dominates: true,
			reader_assets_considered: false,
			book_a: {
				content_chars: 110_000,
				unique_informative_chars: 108_000,
				total_chapters: 12,
				informative_chapters: 12,
				unique_informative_chapters: 12,
				repeated_informative_chapters: 0,
				informative_chapter_ratio: 1,
				unique_informative_ratio: 1,
				word_count: 120_000,
				metadata_quality: 2,
				format_quality: 1,
				file_size_bytes: 1200,
				text_integrity_score: 1,
			},
			book_b: {
				content_chars: 210_000,
				unique_informative_chars: 205_000,
				total_chapters: 20,
				informative_chapters: 20,
				unique_informative_chapters: 20,
				repeated_informative_chapters: 0,
				informative_chapter_ratio: 1,
				unique_informative_ratio: 1,
				word_count: 220_000,
				metadata_quality: 4,
				format_quality: 4,
				file_size_bytes: 2400,
				text_integrity_score: 0.999,
			},
		},
		unique_chapters_a: [10, 11],
		unique_chapters_b: [1, 2, 3, 4, 5, 6, 7, 8, 9],
		book_a_layout_hash: 'layout-a',
		book_b_layout_hash: 'layout-b',
		algorithm_version: 4,
	},
	created_at: '2026-07-13T00:00:00Z',
	updated_at: '2026-07-13T00:00:00Z',
	book_a: {
		id: 'book-a',
		title: '北境旧版',
		author: '林川',
		format: 'txt',
		file_size: 1200,
		word_count: 120_000,
		chapter_count: 12,
		cover_path: null,
	},
	book_b: {
		id: 'book-b',
		title: '北境完整版',
		author: '林川',
		format: 'epub',
		file_size: 2400,
		word_count: 220_000,
		chapter_count: 20,
		cover_path: null,
	},
};

const semanticPair: DuplicatePair = {
	...pair,
	id: 'pair-semantic',
	relation: 'semantic_relation',
	confidence: 0.72,
	semantic_score: 0.81,
	shared_chapters: 0,
	coverage_a: 0,
	coverage_b: 0,
	character_coverage_a: 0,
	character_coverage_b: 0,
	longest_contiguous_run: 0,
	order_score: 0,
	evidence: {
		...pair.evidence,
		semantic_hits: 7,
		semantic: {
			score: 0.82,
			independent_chunk_matches: 7,
			independent_chapter_matches: 3,
			ordered_chapter_matches: [
				{ chapter_a_index: 1, chapter_b_index: 4, score: 0.91 },
				{ chapter_a_index: 3, chapter_b_index: 7, score: 0.86 },
				{ chapter_a_index: 6, chapter_b_index: 10, score: 0.83 },
			],
			matched_chapters_a: 3,
			matched_chapters_b: 3,
			order_score: 0.75,
			sampled_chapters_a: 4,
			sampled_chapters_b: 5,
			sample_coverage_a: 0.75,
			sample_coverage_b: 0.6,
			book_chapters_a: 12,
			book_chapters_b: 20,
			observed_book_coverage_a: 0.25,
			observed_book_coverage_b: 0.15,
		},
	},
	book_a: { ...pair.book_a, id: 'book-semantic-a', title: '雾城原作' },
	book_b: { ...pair.book_b, id: 'book-semantic-b', title: '雾城译写本' },
};

const scan: DuplicateScan = {
	id: 'scan-1',
	library_id: null,
	task_id: 'task-1',
	include_semantic: false,
	algorithm_version: 1,
	status: 'completed',
	progress: 100,
	progress_message: 'completed',
	books_total: 1200,
	books_processed: 1200,
	chapters_processed: 48_000,
	candidates_found: 31,
	pairs_found: 8,
	exact_pairs: 2,
	contained_pairs: 4,
	semantic_pairs: 2,
	error_message: null,
	started_at: '2026-07-13T00:00:00Z',
	completed_at: '2026-07-13T00:10:00Z',
	created_at: '2026-07-13T00:00:00Z',
};

describe('duplicate detection workbench page', () => {
	beforeEach(() => {
		vi.spyOn(api, 'getLibraries').mockResolvedValue([]);
		vi.spyOn(api, 'getLatestDuplicateScan').mockResolvedValue(scan);
		vi.spyOn(api, 'getDuplicatePairs').mockImplementation(async (filters) => ({
			items: filters?.candidate_kind === 'semantic' ? [semanticPair] : [pair],
			total: 1,
			limit: 20,
			offset: 0,
		}));
		vi.spyOn(api, 'getExactFileDiscoveries').mockResolvedValue({
			items: [{
				id: 'discovery-1',
				library_id: 'library-1',
				matched_book_id: 'book-a',
				matched_book_title: '北境旧版',
				matched_book_author: '林舟',
				matched_book_format: 'epub',
				source_kind: 'upload',
				source_path: '北境旧版-重复.epub',
				file_hash: 'abc123',
				file_size_bytes: 1024,
				first_seen_at: '2026-07-13T00:00:00Z',
				last_seen_at: '2026-07-13T00:00:00Z',
				seen_count: 2,
			}],
			total: 1,
			limit: 5,
			offset: 0,
		});
		vi.spyOn(api, 'getDuplicatePair').mockResolvedValue({
			...pair,
			chapter_matches: [{
				id: 'match-1',
				match_type: 'conservative',
				similarity: 0.94,
				shared_fingerprints: 32,
				alignment_group: null,
				segment_ordinal: null,
				chapter_a_start: null,
				chapter_a_end: null,
				chapter_b_start: null,
				chapter_b_end: null,
				matched_chars: 0,
				chapter_a_id: 'chapter-a-1',
				chapter_a_index: 0,
				chapter_a_title: '雪夜',
				chapter_b_id: 'chapter-b-1',
				chapter_b_index: 0,
				chapter_b_title: '雪夜归人',
			}],
			chapter_matches_total: 1,
			chapter_matches_limit: 50,
			chapter_matches_offset: 0,
			matched_indices_a: [0],
			matched_indices_b: [0],
		});
	});

	afterEach(() => {
		cleanup();
		vi.restoreAllMocks();
	});

	it('explains overlap and exposes scanning, filtering, and evidence actions', async () => {
		render(DuplicatePageHarness);

		expect(await screen.findByRole('heading', { name: '重复检测工作台' })).toBeInTheDocument();
		expect(screen.getByRole('tab', { name: '内容重复候选' })).toHaveAttribute('aria-selected', 'true');
		expect(screen.getByRole('tab', { name: '语义相关候选' })).toBeInTheDocument();
		expect(await screen.findByText('北境旧版')).toBeInTheDocument();
		expect(screen.getByText('共同 12 章')).toBeInTheDocument();
		expect(screen.getByLabelText('A 版章节轨道：共同 12 章，独有 0 章，覆盖 100%')).toBeInTheDocument();
		expect(screen.getByRole('button', { name: '开始全库扫描' })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: '查看章节证据' })).toBeInTheDocument();
		expect(screen.getByLabelText('按关系筛选')).toBeInTheDocument();
		expect(screen.getByLabelText('按处理状态筛选')).toBeInTheDocument();
		expect(await screen.findByText('北境旧版-重复.epub')).toBeInTheDocument();
		expect(screen.getByText('命中《北境旧版》 · EPUB')).toBeInTheDocument();
		expect(api.getDuplicatePairs).toHaveBeenCalledWith({
			candidate_kind: 'content',
			relation: undefined,
			status: 'pending',
			library_id: undefined,
			limit: 20,
			offset: 0,
		});
	});

	it.each([
		['recovering', 'running', '正在恢复扫描'],
		['retrying', 'running', '扫描将在稍后重试'],
		['failed', 'failed', '扫描失败'],
		['fingerprinting', 'running', '正在计算内容指纹'],
		['candidate_generation', 'running', '正在生成重复候选'],
		['verifying', 'running', '正在验证章节关系'],
		['completed', 'completed', '扫描已完成'],
	] as const)('localizes the %s scan phase code', async (phase, status, expected) => {
		vi.mocked(api.getLatestDuplicateScan).mockResolvedValue({
			...scan,
			status,
			progress_message: phase,
		});

		render(DuplicatePageHarness);

		expect(await screen.findByText(expected)).toBeInTheDocument();
		expect(screen.queryByText(phase)).not.toBeInTheDocument();
	});

	it('uses a localized fallback when a scan has no current phase', async () => {
		vi.mocked(api.getLatestDuplicateScan).mockResolvedValue({
			...scan,
			status: 'running',
			progress_message: null,
		});

		render(DuplicatePageHarness);

		expect(await screen.findByText('扫描状态已更新')).toBeInTheDocument();
	});

	it('isolates semantic candidates in a manual-review-only tab without changing the URL', async () => {
		const locationBefore = `${window.location.pathname}${window.location.search}${window.location.hash}`;
		render(DuplicatePageHarness);

		expect(await screen.findByText('北境旧版')).toBeInTheDocument();
		expect(screen.queryByText('雾城原作')).not.toBeInTheDocument();
		expect(screen.queryByText('语义相关候选仅供人工复核')).not.toBeInTheDocument();

		await fireEvent.click(screen.getByRole('tab', { name: '语义相关候选' }));

		await waitFor(() => {
			expect(api.getDuplicatePairs).toHaveBeenLastCalledWith({
				candidate_kind: 'semantic',
				relation: undefined,
				status: 'pending',
				library_id: undefined,
				limit: 20,
				offset: 0,
			});
		});
		expect(await screen.findByText('雾城原作')).toBeInTheDocument();
		expect(screen.queryByText('北境旧版')).not.toBeInTheDocument();
		expect(screen.getByText('语义相关候选仅供人工复核')).toBeInTheDocument();
		expect(screen.getByText(/系统不会自动合并这些候选/)).toBeInTheDocument();
		expect(screen.queryByLabelText('按关系筛选')).not.toBeInTheDocument();
		expect(screen.getByLabelText('按处理状态筛选')).toBeInTheDocument();
		expect(`${window.location.pathname}${window.location.search}${window.location.hash}`).toBe(locationBefore);
	});

	it('shows nested semantic evidence instead of zero-valued content overlap metrics', async () => {
		render(DuplicatePageHarness);

		await fireEvent.click(screen.getByRole('tab', { name: '语义相关候选' }));

		expect(await screen.findByText('语义相似度 81%')).toBeInTheDocument();
		expect(screen.getByText('独立章节命中 3')).toBeInTheDocument();
		expect(screen.getByText('独立文本块命中 7')).toBeInTheDocument();
		expect(screen.getByText('有序章对 3')).toBeInTheDocument();
		expect(screen.getByText('章序一致 75%')).toBeInTheDocument();
		expect(screen.getByText('采样覆盖 A 75% · B 60%')).toBeInTheDocument();
		expect(screen.getByText('A 第 2 章 ↔ B 第 5 章 · 91%')).toBeInTheDocument();
		expect(screen.getByText('A 第 4 章 ↔ B 第 8 章 · 86%')).toBeInTheDocument();
		expect(screen.getByText('A 第 7 章 ↔ B 第 11 章 · 83%')).toBeInTheDocument();
		expect(screen.queryByText('共同 0 章')).not.toBeInTheDocument();
		expect(screen.queryByText('最长连续 0 章')).not.toBeInTheDocument();
		expect(screen.queryAllByText('章节覆盖 0% · 字符覆盖 0%')).toHaveLength(0);
	});

	it('resets candidate pagination when the reviewer changes tabs', async () => {
		vi.mocked(api.getDuplicatePairs).mockImplementation(async (filters) => ({
			items: filters?.candidate_kind === 'semantic' ? [semanticPair] : [pair],
			total: 21,
			limit: 20,
			offset: filters?.offset ?? 0,
		}));
		render(DuplicatePageHarness);

		await fireEvent.click(await screen.findByRole('button', { name: '下一页' }));
		await waitFor(() => {
			expect(api.getDuplicatePairs).toHaveBeenLastCalledWith(expect.objectContaining({
				candidate_kind: 'content',
				offset: 20,
			}));
		});

		await fireEvent.click(screen.getByRole('tab', { name: '语义相关候选' }));
		await waitFor(() => {
			expect(api.getDuplicatePairs).toHaveBeenLastCalledWith(expect.objectContaining({
				candidate_kind: 'semantic',
				offset: 0,
			}));
		});
	});

	it('keeps semantic and content relations inside their respective candidate tabs', async () => {
		vi.mocked(api.getDuplicatePairs).mockResolvedValue({
			items: [pair, semanticPair],
			total: 2,
			limit: 20,
			offset: 0,
		});
		render(DuplicatePageHarness);

		expect(await screen.findByText('北境旧版')).toBeInTheDocument();
		expect(screen.queryByText('雾城原作')).not.toBeInTheDocument();

		await fireEvent.click(screen.getByRole('tab', { name: '语义相关候选' }));
		expect(await screen.findByText('雾城原作')).toBeInTheDocument();
		expect(screen.queryByText('北境旧版')).not.toBeInTheDocument();
	});

	it('loads chapter text diff only after the reviewer expands it', async () => {
		const diffSpy = vi.spyOn(api, 'getDuplicateMatchDiff').mockResolvedValue({
			pair_id: 'pair-1',
			match_id: 'match-1',
			chapter_a: { id: 'chapter-a-1', title: '雪夜', character_count: 2100 },
			chapter_b: { id: 'chapter-b-1', title: '雪夜归人', character_count: 2240 },
			changes: [{ tag: 'insert', value: '他终于回来了。' }],
			ratio: 0.94,
			truncated: false,
		});
		render(DuplicatePageHarness);

		await fireEvent.click(await screen.findByRole('button', { name: '查看章节证据' }));
		expect(await screen.findByRole('dialog')).toHaveClass(
			'data-[side=right]:w-full',
			'data-[side=right]:sm:max-w-3xl',
		);
		expect(screen.getByRole('region', { name: '主版本推荐依据' })).toBeInTheDocument();
		expect(screen.getByText('建议保留 B 版')).toBeInTheDocument();
		expect(screen.getByText(/不读取任何用户的阅读记录/)).toBeInTheDocument();
		expect(screen.getByRole('region', { name: '章节边界发生变化' })).toBeInTheDocument();
		expect(screen.getByText('第 1–10 章')).toBeInTheDocument();
		expect(screen.getByText('多章合为一章 · 100,000 字 · 10 段')).toBeInTheDocument();
		const diffToggle = await screen.findByText('查看文本差异');
		expect(diffSpy).not.toHaveBeenCalled();

		await fireEvent.click(diffToggle);

		await waitFor(() => expect(diffSpy).toHaveBeenCalledWith('pair-1', 'match-1'));
		expect(await screen.findByText('+ 他终于回来了。')).toBeInTheDocument();
	});
});
