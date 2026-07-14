export type DeepAnalysisTaskStatus =
	| 'queued'
	| 'running'
	| 'completed'
	| 'failed'
	| 'cancelled'
	| 'dead_letter'
	| 'retrying';

export type DeepAnalysisTask = {
	id: string;
	kind: string;
	status: DeepAnalysisTaskStatus | string;
	progress: number;
	progress_message: string | null;
	error_message: string | null;
	created_at: string;
};

export type DeepAnalysisOverview = {
	chapter_summaries: number;
	sentiment_arcs: number;
	foreshadowing_total: number;
	foreshadowing_unresolved: number;
	macro_windows: number;
	has_deep_analysis: boolean;
};

export type DeepAnalysisState = 'missing' | 'queued' | 'running' | 'failed' | 'partial' | 'ready';

export type DeepAnalysisStatus = {
	state: DeepAnalysisState;
	progress: number;
	message: string;
	missingLabels: string[];
	activeTaskId: string | null;
	canSubmit: boolean;
	isPolling: boolean;
};

const DEEP_ANALYSIS_TASK_KINDS = new Set(['deep_analysis', 'sentiment_arc', 'track_foreshadowing']);
const ACTIVE_TASK_STATUSES = new Set(['queued', 'running', 'retrying']);
const FAILED_TASK_STATUSES = new Set(['failed', 'dead_letter', 'cancelled']);

export function getDeepAnalysisStatus({
	overview,
	chapterCount,
	tasks,
}: {
	overview: DeepAnalysisOverview | null;
	chapterCount?: number | null;
	tasks: DeepAnalysisTask[];
}): DeepAnalysisStatus {
	const relatedTasks = tasks
		.filter((task) => DEEP_ANALYSIS_TASK_KINDS.has(task.kind))
		.toSorted((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at));
	const activeTask = relatedTasks.find((task) => ACTIVE_TASK_STATUSES.has(task.status));
	const latestFailedTask = relatedTasks.find((task) => FAILED_TASK_STATUSES.has(task.status));
	const missingLabels = getMissingLabels(overview, chapterCount);

	if (activeTask) {
		const state = activeTask.status === 'running' ? 'running' : 'queued';
		return {
			state,
			progress: clampProgress(activeTask.progress),
			message: activeTask.progress_message ?? (state === 'running' ? '深度分析正在生成' : '深度分析任务已排队'),
			missingLabels,
			activeTaskId: activeTask.id,
			canSubmit: false,
			isPolling: true,
		};
	}

	if (latestFailedTask && missingLabels.length > 0) {
		return {
			state: 'failed',
			progress: clampProgress(latestFailedTask.progress),
			message: latestFailedTask.error_message ?? '深度分析任务未完成',
			missingLabels,
			activeTaskId: latestFailedTask.id,
			canSubmit: true,
			isPolling: false,
		};
	}

	if (!overview?.has_deep_analysis) {
		return {
			state: 'missing',
			progress: 0,
			message: '尚未生成深度分析结果',
			missingLabels,
			activeTaskId: null,
			canSubmit: true,
			isPolling: false,
		};
	}

	if (missingLabels.length > 0) {
		return {
			state: 'partial',
			progress: completionProgress(missingLabels),
			message: '深度分析结果尚未完整',
			missingLabels,
			activeTaskId: null,
			canSubmit: true,
			isPolling: false,
		};
	}

	return {
		state: 'ready',
		progress: 100,
		message: '深度分析结果已生成',
		missingLabels: [],
		activeTaskId: null,
		canSubmit: true,
		isPolling: false,
	};
}

function getMissingLabels(overview: DeepAnalysisOverview | null, chapterCount?: number | null): string[] {
	if (!overview) return ['章节摘要', '情感曲线', '宏观分析'];

	const missing: string[] = [];
	const expectedChapters = Math.max(0, chapterCount ?? 0);
	const needsChapterCoverage = expectedChapters > 0;

	if (overview.chapter_summaries === 0 || (needsChapterCoverage && overview.chapter_summaries < expectedChapters)) {
		missing.push('章节摘要');
	}
	if (overview.sentiment_arcs === 0 || (needsChapterCoverage && overview.sentiment_arcs < Math.min(overview.chapter_summaries, expectedChapters))) {
		missing.push('情感曲线');
	}
	if (overview.macro_windows === 0 && (overview.chapter_summaries >= 3 || expectedChapters >= 3)) {
		missing.push('宏观分析');
	}

	return missing;
}

function completionProgress(missingLabels: string[]): number {
	const totalSections = 3;
	return Math.round(((totalSections - missingLabels.length) / totalSections) * 100);
}

function clampProgress(progress: number): number {
	if (!Number.isFinite(progress)) return 0;
	return Math.max(0, Math.min(100, Math.round(progress)));
}
