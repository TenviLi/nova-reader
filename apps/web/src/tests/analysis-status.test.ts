import { describe, expect, it } from 'vitest';

import { getDeepAnalysisStatus } from '$lib/utils/analysisStatus';

describe('getDeepAnalysisStatus', () => {
	it('reports missing analysis requirements before a task exists', () => {
		const status = getDeepAnalysisStatus({
			overview: {
				chapter_summaries: 0,
				sentiment_arcs: 0,
				foreshadowing_total: 0,
				foreshadowing_unresolved: 0,
				macro_windows: 0,
				has_deep_analysis: false,
			},
			chapterCount: 24,
			tasks: [],
		});

		expect(status.state).toBe('missing');
		expect(status.progress).toBe(0);
		expect(status.missingLabels).toEqual(['章节摘要', '情感曲线', '宏观分析']);
	});

	it('uses the newest running deep-analysis task as the visible state', () => {
		const status = getDeepAnalysisStatus({
			overview: {
				chapter_summaries: 5,
				sentiment_arcs: 0,
				foreshadowing_total: 0,
				foreshadowing_unresolved: 0,
				macro_windows: 0,
				has_deep_analysis: true,
			},
			chapterCount: 24,
			tasks: [
				{
					id: 'old',
					kind: 'deep_analysis',
					status: 'completed',
					progress: 100,
					progress_message: null,
					error_message: null,
					created_at: '2026-01-01T00:00:00Z',
				},
				{
					id: 'new',
					kind: 'deep_analysis',
					status: 'running',
					progress: 42,
					progress_message: 'Analyzing chapter 10/24',
					error_message: null,
					created_at: '2026-01-02T00:00:00Z',
				},
			],
		});

		expect(status.state).toBe('running');
		expect(status.progress).toBe(42);
		expect(status.message).toBe('Analyzing chapter 10/24');
		expect(status.activeTaskId).toBe('new');
	});

	it('reports ready when analysis results are complete and no active task exists', () => {
		const status = getDeepAnalysisStatus({
			overview: {
				chapter_summaries: 24,
				sentiment_arcs: 24,
				foreshadowing_total: 3,
				foreshadowing_unresolved: 1,
				macro_windows: 1,
				has_deep_analysis: true,
			},
			chapterCount: 24,
			tasks: [],
		});

		expect(status.state).toBe('ready');
		expect(status.progress).toBe(100);
		expect(status.missingLabels).toEqual([]);
	});
});
