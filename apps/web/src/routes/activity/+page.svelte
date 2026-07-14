<script lang="ts">
	import { api } from '$services/api';
	import { onMount } from 'svelte';

	let loading = $state(true);
	let viewMode = $state<'timeline' | 'stats' | 'heatmap'>('timeline');
	let timeRange = $state<'week' | 'month' | 'year'>('month');

	let activities = $state.raw<Array<{
		id: string;
		type: 'reading' | 'annotation' | 'completion' | 'import';
		book_title: string;
		book_id: string;
		chapter_title?: string;
		description: string;
		created_at: string;
		duration_minutes?: number;
		pages_read?: number;
	}>>([]);

	let stats = $state({
		totalBooksRead: 0,
		totalReadingTime: 0,
		totalAnnotations: 0,
		avgDailyMinutes: 0,
		longestStreak: 0,
		currentStreak: 0,
		booksThisMonth: 0,
		pagesThisWeek: 0,
	});

	let heatmapData = $state.raw<Array<{ date: string; total_minutes: number; total_words: number; sessions_count: number }>>([]);

	onMount(async () => {
		try {
			const [actList, statsData, heatmap] = await Promise.all([
				api.getActivities({ limit: 50 }),
				api.getReadingStats(timeRange),
				api.getReadingHeatmap(),
			]);
			activities = actList;
			stats = statsData;
			heatmapData = heatmap;
		} catch {
			// Will show empty state
		} finally {
			loading = false;
		}
	});

	function timeAgo(dateStr: string): string {
		const diff = Date.now() - new Date(dateStr).getTime();
		const minutes = Math.floor(diff / 60000);
		if (minutes < 60) return `${minutes}分钟前`;
		const hours = Math.floor(minutes / 60);
		if (hours < 24) return `${hours}小时前`;
		const days = Math.floor(hours / 24);
		if (days < 30) return `${days}天前`;
		return new Date(dateStr).toLocaleDateString('zh-CN');
	}

	function formatDuration(minutes: number): string {
		if (minutes < 60) return `${minutes}分钟`;
		const h = Math.floor(minutes / 60);
		const m = minutes % 60;
		return m > 0 ? `${h}小时${m}分钟` : `${h}小时`;
	}

	function getActivityIcon(type: string): string {
		switch (type) {
			case 'reading': return '📖';
			case 'annotation': return '✏️';
			case 'completion': return '🎉';
			case 'import': return '📥';
			default: return '📝';
		}
	}

	// Generate heatmap grid (last 365 days)
	function getHeatmapGrid() {
		const grid: Array<{ date: string; count: number; level: number }> = [];
		const today = new Date();
		for (let i = 364; i >= 0; i--) {
			const d = new Date(today);
			d.setDate(d.getDate() - i);
			const dateStr = d.toISOString().split('T')[0];
			const entry = heatmapData.find(h => h.date === dateStr);
			const count = entry?.sessions_count ?? 0;
			const level = count === 0 ? 0 : count <= 2 ? 1 : count <= 5 ? 2 : count <= 10 ? 3 : 4;
			grid.push({ date: dateStr, count, level });
		}
		return grid;
	}
</script>

<svelte:head>
	<title>Nova Reader — 阅读活动</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">阅读活动</h1>
			<p class="mt-1 text-sm text-ink-400">追踪你的阅读旅程</p>
		</div>
		<div class="flex gap-1 rounded-lg bg-ink-900/50 p-1">
			{#each [{ v: 'timeline', l: '时间线' }, { v: 'stats', l: '统计' }, { v: 'heatmap', l: '热力图' }] as tab}
				<button
					onclick={() => viewMode = tab.v as typeof viewMode}
					class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {viewMode === tab.v ? 'bg-accent-500/20' : ''}"
					class:text-accent-300={viewMode === tab.v}
					class:text-ink-400={viewMode !== tab.v}
				>
					{tab.l}
				</button>
			{/each}
		</div>
	</div>

	{#if loading}
		<div class="grid grid-cols-4 gap-4">
			{#each Array(4) as _}
				<div class="h-24 rounded-xl bg-ink-900/30 animate-pulse"></div>
			{/each}
		</div>
	{:else}
		<!-- Stats Cards -->
		<div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<div class="text-2xl font-bold text-accent-400">{stats.totalBooksRead}</div>
				<div class="text-xs text-ink-400 mt-1">已读书籍</div>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<div class="text-2xl font-bold text-emerald-400">{formatDuration(stats.totalReadingTime)}</div>
				<div class="text-xs text-ink-400 mt-1">总阅读时长</div>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<div class="text-2xl font-bold text-amber-400">{stats.currentStreak}</div>
				<div class="text-xs text-ink-400 mt-1">当前连续天数</div>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<div class="text-2xl font-bold text-violet-400">{stats.totalAnnotations}</div>
				<div class="text-xs text-ink-400 mt-1">批注总数</div>
			</div>
		</div>

		{#if viewMode === 'timeline'}
			<!-- Timeline -->
			<div class="space-y-0">
				{#each activities as activity, i}
					<div class="relative flex gap-4 pb-6">
						<!-- Vertical line -->
						{#if i < activities.length - 1}
							<div class="absolute left-5 top-10 bottom-0 w-px bg-ink-800/50"></div>
						{/if}
						<!-- Icon -->
						<div class="relative z-10 flex h-10 w-10 shrink-0 items-center justify-center rounded-full border border-ink-800/50 bg-ink-900/50 text-lg">
							{getActivityIcon(activity.type)}
						</div>
						<!-- Content -->
						<div class="flex-1 pt-1">
							<div class="flex items-baseline justify-between">
								<a href="/library/{activity.book_id}" class="text-sm font-medium text-ink-200 hover:text-accent-400 transition-colors">
									{activity.book_title}
								</a>
								<span class="text-xs text-ink-500">{timeAgo(activity.created_at)}</span>
							</div>
							<p class="mt-0.5 text-xs text-ink-400">{activity.description}</p>
							{#if activity.duration_minutes}
								<span class="mt-1 inline-block rounded-md bg-ink-800/30 px-2 py-0.5 text-[10px] text-ink-400">
									⏱ {formatDuration(activity.duration_minutes)}
								</span>
							{/if}
						</div>
					</div>
				{/each}
				{#if activities.length === 0}
					<div class="py-16 text-center text-ink-500">
						<p class="text-4xl mb-3">📚</p>
						<p>还没有阅读记录，开始你的第一本书吧</p>
					</div>
				{/if}
			</div>

		{:else if viewMode === 'stats'}
			<!-- Detailed Stats -->
			<div class="grid gap-4 md:grid-cols-2">
				<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-6">
					<h3 class="text-sm font-medium text-ink-300 mb-4">阅读习惯</h3>
					<div class="space-y-3">
						<div class="flex justify-between text-sm">
							<span class="text-ink-400">日均阅读</span>
							<span class="text-ink-200">{formatDuration(stats.avgDailyMinutes)}</span>
						</div>
						<div class="flex justify-between text-sm">
							<span class="text-ink-400">最长连续</span>
							<span class="text-ink-200">{stats.longestStreak} 天</span>
						</div>
						<div class="flex justify-between text-sm">
							<span class="text-ink-400">本月完成</span>
							<span class="text-ink-200">{stats.booksThisMonth} 本</span>
						</div>
						<div class="flex justify-between text-sm">
							<span class="text-ink-400">本周阅读</span>
							<span class="text-ink-200">{stats.pagesThisWeek} 页</span>
						</div>
					</div>
				</div>
				<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-6">
					<h3 class="text-sm font-medium text-ink-300 mb-4">成就</h3>
					<div class="grid grid-cols-3 gap-3">
						{#each [
							{ icon: '🌟', label: '初次阅读', done: true },
							{ icon: '📚', label: '十本达成', done: stats.totalBooksRead >= 10 },
							{ icon: '🔥', label: '七天连续', done: stats.longestStreak >= 7 },
							{ icon: '✍️', label: '百条笔记', done: stats.totalAnnotations >= 100 },
							{ icon: '⚡', label: '30天连续', done: stats.longestStreak >= 30 },
							{ icon: '🏆', label: '读完50本', done: stats.totalBooksRead >= 50 },
						] as achievement}
							<div class="flex flex-col items-center gap-1 rounded-lg p-2" class:opacity-30={!achievement.done}>
								<span class="text-2xl">{achievement.icon}</span>
								<span class="text-[10px] text-ink-400 text-center">{achievement.label}</span>
							</div>
						{/each}
					</div>
				</div>
			</div>

		{:else if viewMode === 'heatmap'}
			<!-- GitHub-style Heatmap -->
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-6">
				<h3 class="text-sm font-medium text-ink-300 mb-4">阅读热力图 (过去一年)</h3>
				<div class="flex flex-wrap gap-[3px]">
					{#each getHeatmapGrid() as cell}
						<div
							class="h-3 w-3 rounded-sm {cell.level === 0 ? 'bg-ink-800/30' : ''} {cell.level === 1 ? 'bg-accent-900/50' : ''} {cell.level === 2 ? 'bg-accent-700/60' : ''} {cell.level === 3 ? 'bg-accent-500/70' : ''}"
							class:bg-accent-400={cell.level === 4}
							title="{cell.date}: {cell.count} 次活动"
						></div>
					{/each}
				</div>
				<div class="mt-3 flex items-center gap-2 text-xs text-ink-500">
					<span>少</span>
					<div class="h-3 w-3 rounded-sm bg-ink-800/30"></div>
					<div class="h-3 w-3 rounded-sm bg-accent-900/50"></div>
					<div class="h-3 w-3 rounded-sm bg-accent-700/60"></div>
					<div class="h-3 w-3 rounded-sm bg-accent-500/70"></div>
					<div class="h-3 w-3 rounded-sm bg-accent-400"></div>
					<span>多</span>
				</div>
			</div>
		{/if}
	{/if}
</div>
