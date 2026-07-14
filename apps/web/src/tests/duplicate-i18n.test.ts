import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

import * as m from '$lib/paraglide/messages.js';

const appRoot = process.cwd();
const duplicateSources = [
	'src/routes/library/duplicates/+page.svelte',
	'src/lib/components/duplicates/DuplicatePairCard.svelte',
	'src/lib/components/duplicates/SemanticEvidencePanel.svelte',
	'src/lib/components/duplicates/ChapterOverlapTrack.svelte',
	'src/lib/components/duplicates/ChapterMatchDiff.svelte',
	'src/lib/components/duplicates/GroupedAlignmentPanel.svelte',
];

function readAppFile(path: string): string {
	return readFileSync(resolve(appRoot, path), 'utf8');
}

describe('duplicate review translations', () => {
	it('renders translated static and interpolated messages', () => {
		expect(m.duplicates_title({}, { locale: 'zh' })).toBe('重复检测工作台');
		expect(m.duplicates_title({}, { locale: 'en' })).toBe('Duplicate Review');
		expect(m.duplicates_range({ start: 1, end: 20, total: '1,000' }, { locale: 'zh' })).toBe(
			'当前第 1–20 条，共 1,000 条。',
		);
		expect(m.duplicates_range({ start: 1, end: 20, total: '1,000' }, { locale: 'en' })).toBe(
			'Showing 1–20 of 1,000.',
		);
		expect(m.duplicates_resolution_dismiss({}, { locale: 'en' })).toBe(
			'Marked as not a duplicate',
		);
		expect(m.duplicates_chapter_count({ count: 1 }, { locale: 'en' })).toBe('Chapters: 1');
		expect(m.duplicates_content_candidates_tab({}, { locale: 'zh' })).toBe('内容重复候选');
		expect(m.duplicates_content_candidates_tab({}, { locale: 'en' })).toBe(
			'Content duplicate candidates',
		);
		expect(m.duplicates_semantic_review_title({}, { locale: 'zh' })).toBe(
			'语义相关候选仅供人工复核',
		);
		expect(m.duplicates_semantic_review_title({}, { locale: 'en' })).toBe(
			'Semantic candidates require manual review',
		);
		expect(m.duplicates_semantic_score({ score: 81 }, { locale: 'zh' })).toBe(
			'语义相似度 81%',
		);
		expect(m.duplicates_semantic_score({ score: 81 }, { locale: 'en' })).toBe(
			'Semantic similarity 81%',
		);
		expect(
			m.duplicates_semantic_sample_coverage({ a: '75%', b: '60%' }, { locale: 'zh' }),
		).toBe('采样覆盖 A 75% · B 60%');
		expect(
			m.duplicates_semantic_sample_coverage({ a: '75%', b: '60%' }, { locale: 'en' }),
		).toBe('Sample coverage A 75% · B 60%');
		expect(
			m.duplicates_boundary_group_metrics(
				{ shape: '多章合为一章', characters: '100,000', segments: '10' },
				{ locale: 'zh' },
			),
		).toBe('多章合为一章 · 100,000 字 · 10 段');
	});

	it('authors every stable scan phase in Chinese and English', () => {
		const phases = [
			[m.duplicates_scan_phase_recovering, '正在恢复扫描', 'Recovering scan'],
			[m.duplicates_scan_phase_retrying, '扫描将在稍后重试', 'Scan will retry shortly'],
			[m.duplicates_scan_phase_failed, '扫描失败', 'Scan failed'],
			[m.duplicates_scan_phase_fingerprinting, '正在计算内容指纹', 'Fingerprinting book content'],
			[m.duplicates_scan_phase_candidate_generation, '正在生成重复候选', 'Generating duplicate candidates'],
			[m.duplicates_scan_phase_verifying, '正在验证章节关系', 'Verifying chapter relationships'],
			[m.duplicates_scan_phase_completed, '扫描已完成', 'Scan completed'],
		] as const;

		for (const [message, zh, en] of phases) {
			expect(message({}, { locale: 'zh' })).toBe(zh);
			expect(message({}, { locale: 'en' })).toBe(en);
		}
	});

	it('falls back to the Chinese base locale while Japanese messages are not authored yet', () => {
		expect(m.duplicates_title({}, { locale: 'ja' })).toBe(
			m.duplicates_title({}, { locale: 'zh' }),
		);
	});

	it('keeps every message call backed by both authored locale files', () => {
		const en = JSON.parse(readAppFile('messages/en.json')) as Record<string, string>;
		const zh = JSON.parse(readAppFile('messages/zh.json')) as Record<string, string>;
		const usedKeys = new Set(
			duplicateSources.flatMap((path) =>
				[...readAppFile(path).matchAll(/\bm\.([a-z0-9_]+)\(/g)].map((match) => match[1]),
			),
		);

		for (const key of usedKeys) {
			expect(en, `missing English message: ${key}`).toHaveProperty(key);
			expect(zh, `missing Chinese message: ${key}`).toHaveProperty(key);
		}
	});

	it('contains no user-facing Chinese literals in duplicate review Svelte sources', () => {
		for (const path of duplicateSources) {
			expect(readAppFile(path), path).not.toMatch(/[\u3400-\u9fff]/u);
		}
	});

	it('uses the Paraglide v2 project schema and message-format compiler', () => {
		const settings = JSON.parse(readAppFile('project.inlang/settings.json')) as {
			baseLocale?: string;
			locales?: string[];
			modules?: string[];
		};

		expect(settings.baseLocale).toBe('zh');
		expect(settings.locales).toEqual(['zh', 'en', 'ja']);
		expect(settings.modules).toEqual(
			expect.arrayContaining([
				expect.stringContaining('@inlang/plugin-message-format'),
				expect.stringContaining('@inlang/plugin-m-function-matcher'),
			]),
		);
	});
});
