<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import { api } from '$services/api';
	import { BookOpen, Clock, Target, TrendingUp, BarChart3, Calendar, Award } from 'lucide-svelte';

	let activeTab = $state<'stats' | 'activity' | 'history'>('stats');

	type ReadingStats = Awaited<ReturnType<typeof api.getReadingStats>>;
	type HeatmapEntry = Awaited<ReturnType<typeof api.getReadingHeatmap>>[number];
	type Session = Awaited<ReturnType<typeof api.getReadingSessions>>[number];
	type Goal = Awaited<ReturnType<typeof api.getReadingGoals>>[number];
	type Activity = Awaited<ReturnType<typeof api.getActivities>>[number];

	// Reading statistics
	const stats = createQuery(() => ({
		queryKey: ['reading', 'stats'] as const,
		queryFn: () => api.getReadingStats(),
	}));

	// Reading heatmap data (last 365 days)
	const heatmap = createQuery(() => ({
		queryKey: ['reading', 'heatmap'] as const,
		queryFn: () => api.getReadingHeatmap(),
	}));

	// Recent sessions
	const sessions = createQuery(() => ({
		queryKey: ['reading', 'sessions'] as const,
		queryFn: () => api.getReadingSessions({ limit: 20 }),
	}));

	// Reading goals
	const goals = createQuery(() => ({
		queryKey: ['reading', 'goals'] as const,
		queryFn: () => api.getReadingGoals(),
	}));

	// Activities (for activity tab)
	const activities = createQuery(() => ({
		queryKey: ['activities'] as const,
		queryFn: () => api.getActivities({ limit: 50 }),
	}));

	// Type-safe data accessors
	let statsData = $derived(stats.data as ReadingStats | undefined);
	let heatmapData = $derived(heatmap.data as HeatmapEntry[] | undefined);
	let sessionsData = $derived(sessions.data as Session[] | undefined);
	let goalsData = $derived(goals.data as Goal[] | undefined);
	let activitiesData = $derived(activities.data as Activity[] | undefined);

	function formatMinutes(mins: number): string {
		if (mins < 60) return `${mins}分钟`;
		const h = Math.floor(mins / 60);
		const m = mins % 60;
		return m > 0 ? `${h}小时${m}分` : `${h}小时`;
	}

	function formatWords(words: number): string {
		if (words >= 10000) return `${(words / 10000).toFixed(1)}万`;
		if (words >= 1000) return `${(words / 1000).toFixed(1)}千`;
		return `${words}`;
	}

	// Generate heatmap grid (52 weeks × 7 days)
	function generateHeatmapGrid(data: Array<{ date: string; total_minutes: number }>) {
		const grid: Array<{ date: string; level: number; minutes: number }> = [];
		const today = new Date();
		const map = new Map(data?.map(d => [d.date, d.total_minutes]) ?? []);

		for (let i = 364; i >= 0; i--) {
			const date = new Date(today);
			date.setDate(date.getDate() - i);
			const dateStr = date.toISOString().split('T')[0];
			const minutes = map.get(dateStr) ?? 0;
			const level = minutes === 0 ? 0 : minutes < 15 ? 1 : minutes < 30 ? 2 : minutes < 60 ? 3 : 4;
			grid.push({ date: dateStr, level, minutes });
		}
		return grid;
	}

	let heatmapGrid = $derived(generateHeatmapGrid(heatmapData ?? []));
</script>

<svelte:head>
	<title>Nova Reader — 阅读统计</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 space-y-8 animate-fade-in">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">统计与活动</h1>
			<p class="mt-1 text-sm text-ink-400">追踪你的阅读旅程</p>
		</div>
		<div class="flex gap-1 rounded-lg bg-ink-900/50 p-1 border border-ink-800/50">
			<button
				onclick={() => activeTab = 'stats'}
				class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {activeTab === 'stats' ? 'bg-accent-500/20 text-accent-300' : 'text-ink-400 hover:text-ink-200'}"
			>统计</button>
			<button
				onclick={() => activeTab = 'history'}
				class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {activeTab === 'history' ? 'bg-accent-500/20 text-accent-300' : 'text-ink-400 hover:text-ink-200'}"
			>阅读历史</button>
			<button
				onclick={() => activeTab = 'activity'}
				class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {activeTab === 'activity' ? 'bg-accent-500/20 text-accent-300' : 'text-ink-400 hover:text-ink-200'}"
			>活动流</button>
		</div>
	</div>

	{#if activeTab === 'stats'}

	<!-- Overview Stats -->
	<div class="grid grid-cols-2 gap-4 md:grid-cols-4">
		<div class="rounded-xl border border-ink-800/50 bg-ink-900/80 p-5">
			<div class="flex items-center gap-2 text-ink-400">
				<BookOpen class="w-4 h-4" />
				<span class="text-xs uppercase tracking-wide">已读完</span>
			</div>
			<p class="mt-2 text-3xl font-bold text-ink-50">
				{statsData?.totalBooksRead ?? 0}
			</p>
			<p class="mt-1 text-xs text-ink-500">本书</p>
		</div>

		<div class="rounded-xl border border-ink-800/50 bg-ink-900/80 p-5">
			<div class="flex items-center gap-2 text-ink-400">
				<Clock class="w-4 h-4" />
				<span class="text-xs uppercase tracking-wide">阅读时长</span>
			</div>
			<p class="mt-2 text-3xl font-bold text-ink-50">
				{formatMinutes(Math.floor((statsData?.totalReadingTime ?? 0) / 60))}
			</p>
			<p class="mt-1 text-xs text-ink-500">累计</p>
		</div>

		<div class="rounded-xl border border-ink-800/50 bg-ink-900/80 p-5">
			<div class="flex items-center gap-2 text-ink-400">
				<TrendingUp class="w-4 h-4" />
				<span class="text-xs uppercase tracking-wide">批注数量</span>
			</div>
			<p class="mt-2 text-3xl font-bold text-ink-50">
				{statsData?.totalAnnotations ?? 0}
			</p>
			<p class="mt-1 text-xs text-ink-500">条</p>
		</div>

		<div class="rounded-xl border border-ink-800/50 bg-ink-900/80 p-5">
			<div class="flex items-center gap-2 text-ink-400">
				<Award class="w-4 h-4" />
				<span class="text-xs uppercase tracking-wide">连续阅读</span>
			</div>
			<p class="mt-2 text-3xl font-bold text-amber-400">
				{statsData?.currentStreak ?? 0}
			</p>
			<p class="mt-1 text-xs text-ink-500">天</p>
		</div>
	</div>

	<!-- Reading Heatmap (GitHub-style) -->
	<div class="rounded-xl border border-ink-800/50 bg-ink-900/80 p-6">
		<div class="flex items-center justify-between mb-4">
			<h2 class="text-lg font-semibold text-ink-100 flex items-center gap-2">
				<Calendar class="w-5 h-5 text-ink-400" />
				阅读热力图
			</h2>
			<div class="flex items-center gap-2 text-xs text-ink-500">
				<span>少</span>
				<div class="flex gap-0.5">
					<div class="w-3 h-3 rounded-sm bg-ink-800"></div>
					<div class="w-3 h-3 rounded-sm bg-emerald-900"></div>
					<div class="w-3 h-3 rounded-sm bg-emerald-700"></div>
					<div class="w-3 h-3 rounded-sm bg-emerald-500"></div>
					<div class="w-3 h-3 rounded-sm bg-emerald-300"></div>
				</div>
				<span>多</span>
			</div>
		</div>

		<!-- Heatmap Grid -->
		<div class="overflow-x-auto">
			<div class="grid grid-flow-col grid-rows-7 gap-[3px]" style="grid-template-columns: repeat(53, 1fr);">
				{#each heatmapGrid as cell}
					<div
						class="w-3 h-3 rounded-sm transition-colors"
						class:bg-ink-800={cell.level === 0}
						class:bg-emerald-900={cell.level === 1}
						class:bg-emerald-700={cell.level === 2}
						class:bg-emerald-500={cell.level === 3}
						class:bg-emerald-300={cell.level === 4}
						title="{cell.date}: {cell.minutes}分钟"
					></div>
				{/each}
			</div>
		</div>
	</div>

	<!-- Two-column layout: Goals + Recent Sessions -->
	<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
		<!-- Reading Goals -->
		<div class="rounded-xl border border-ink-800/50 bg-ink-900/80 p-6">
			<h2 class="text-lg font-semibold text-ink-100 flex items-center gap-2 mb-4">
				<Target class="w-5 h-5 text-ink-400" />
				阅读目标
			</h2>

			{#if goalsData && goalsData.length > 0}
				<div class="space-y-4">
					{#each goalsData as goal}
						{@const percent = Math.min(100, (goal.progress / goal.target) * 100)}
						<div>
							<div class="flex items-center justify-between mb-1">
								<span class="text-sm text-ink-200">{goal.label}</span>
								<span class="text-xs text-ink-400">
									{goal.progress} / {goal.target}
								</span>
							</div>
							<div class="h-2 w-full rounded-full bg-ink-800">
								<div
									class="h-full rounded-full transition-all duration-500"
									class:bg-emerald-500={percent >= 100}
									class:bg-amber-500={percent >= 50 && percent < 100}
									class:bg-ink-600={percent < 50}
									style="width: {percent}%"
								></div>
							</div>
						</div>
					{/each}
				</div>
			{:else}
				<p class="text-sm text-ink-500">暂无阅读目标。前往设置添加。</p>
			{/if}
		</div>

		<!-- Recent Sessions -->
		<div class="rounded-xl border border-ink-800/50 bg-ink-900/80 p-6">
			<h2 class="text-lg font-semibold text-ink-100 flex items-center gap-2 mb-4">
				<BarChart3 class="w-5 h-5 text-ink-400" />
				最近阅读
			</h2>

			{#if sessionsData && sessionsData.length > 0}
				<div class="space-y-3">
					{#each sessionsData.slice(0, 8) as session}
						<div class="flex items-center justify-between py-2 border-b border-ink-800/30 last:border-0">
							<div class="flex-1 min-w-0">
								<p class="text-sm text-ink-200 truncate">{session.book_title}</p>
								<p class="text-xs text-ink-500">
									第{session.start_chapter}章 → 第{session.end_chapter}章
								</p>
							</div>
							<div class="text-right">
								<p class="text-sm text-ink-300">{formatMinutes(Math.floor(session.duration_secs / 60))}</p>
								<p class="text-xs text-ink-500">{formatWords(session.words_read)}字</p>
							</div>
						</div>
					{/each}
				</div>
			{:else}
				<p class="text-sm text-ink-500">暂无阅读记录。开始阅读吧！</p>
			{/if}
		</div>
	</div>

	{:else if activeTab === 'history'}
		<!-- Reading History (all sessions) -->
		<div class="rounded-xl border border-ink-800/50 bg-ink-900/80 p-6">
			<h2 class="text-lg font-semibold text-ink-100 mb-4">阅读历史</h2>
			{#if sessionsData && sessionsData.length > 0}
				<div class="overflow-x-auto">
					<table class="w-full text-sm">
						<thead>
							<tr class="border-b border-ink-800/50 text-left text-xs text-ink-500 uppercase tracking-wide">
								<th class="py-2 pr-4">书籍</th>
								<th class="py-2 pr-4">章节</th>
								<th class="py-2 pr-4">时长</th>
								<th class="py-2 pr-4">字数</th>
								<th class="py-2">日期</th>
							</tr>
						</thead>
						<tbody class="divide-y divide-ink-800/30">
							{#each sessionsData as session}
								<tr class="text-ink-300 hover:bg-ink-800/30">
									<td class="py-2.5 pr-4 max-w-[200px] truncate font-medium text-ink-200">{session.book_title ?? '—'}</td>
									<td class="py-2.5 pr-4 text-ink-400">
										{#if session.start_chapter != null}
											Ch.{session.start_chapter}{session.end_chapter ? ` → ${session.end_chapter}` : ''}
										{:else}
											—
										{/if}
									</td>
									<td class="py-2.5 pr-4">{formatMinutes(Math.floor((session.duration_secs ?? 0) / 60))}</td>
									<td class="py-2.5 pr-4">{formatWords(session.words_read ?? 0)}</td>
									<td class="py-2.5 text-ink-500 text-xs">{new Date(session.started_at).toLocaleDateString('zh-CN')}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{:else}
				<p class="text-sm text-ink-500 text-center py-8">暂无阅读历史记录</p>
			{/if}
		</div>

	{:else}
		<!-- Activity Timeline -->
		{#if activities.isLoading}
			<div class="space-y-4">
				{#each Array(5) as _}
					<div class="flex gap-4">
						<div class="w-10 h-10 rounded-full bg-ink-800/30 animate-pulse"></div>
						<div class="flex-1 space-y-2">
							<div class="h-3 w-1/3 rounded bg-ink-800/30 animate-pulse"></div>
							<div class="h-2 w-2/3 rounded bg-ink-800/20 animate-pulse"></div>
						</div>
					</div>
				{/each}
			</div>
		{:else if activitiesData && activitiesData.length > 0}
			<div class="space-y-0">
				{#each activitiesData as activity, i}
					<div class="relative flex gap-4 pb-6">
						{#if i < activitiesData.length - 1}
							<div class="absolute left-5 top-10 bottom-0 w-px bg-ink-800/50"></div>
						{/if}
						<div class="relative z-10 flex h-10 w-10 shrink-0 items-center justify-center rounded-full border border-ink-800/50 bg-ink-900/50 text-lg">
							{activity.type === 'reading' ? '📖' : activity.type === 'annotation' ? '✏️' : activity.type === 'completion' ? '🎉' : activity.type === 'import' ? '📥' : '📝'}
						</div>
						<div class="flex-1 pt-1">
							<div class="flex items-baseline justify-between">
								<a href="/library/{activity.book_id}" class="text-sm font-medium text-ink-200 hover:text-accent-400 transition-colors">
									{activity.book_title}
								</a>
								<span class="text-xs text-ink-500">{new Date(activity.created_at).toLocaleDateString('zh-CN')}</span>
							</div>
							<p class="mt-0.5 text-xs text-ink-400">{activity.description}</p>
							{#if activity.duration_minutes}
								<span class="mt-1 inline-block rounded-md bg-ink-800/30 px-2 py-0.5 text-[10px] text-ink-400">
									⏱ {activity.duration_minutes < 60 ? `${activity.duration_minutes}分钟` : `${Math.floor(activity.duration_minutes / 60)}小时${activity.duration_minutes % 60}分`}
								</span>
							{/if}
						</div>
					</div>
				{/each}
			</div>
		{:else}
			<div class="py-16 text-center text-ink-500">
				<p class="text-4xl mb-3">📚</p>
				<p>还没有阅读记录，开始你的第一本书吧</p>
			</div>
		{/if}
	{/if}
</div>
