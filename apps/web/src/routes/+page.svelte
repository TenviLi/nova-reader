<script lang="ts">
	import StatsCard from '$components/dashboard/StatsCard.svelte';
	import TaskQueue from '$components/dashboard/TaskQueue.svelte';
	import ReadingActivity from '$components/dashboard/ReadingActivity.svelte';
	import ReadingStreak from '$components/dashboard/ReadingStreak.svelte';
	import ActivityFeed from '$components/dashboard/ActivityFeed.svelte';
	import ContinueReading from '$components/dashboard/ContinueReading.svelte';
	import Recommendations from '$components/dashboard/Recommendations.svelte';
	import ReadingMemories from '$components/dashboard/ReadingMemories.svelte';
	import RecentlyFinished from '$components/dashboard/RecentlyFinished.svelte';
	import NewArrivals from '$components/dashboard/NewArrivals.svelte';
	import SeriesWidget from '$components/dashboard/SeriesWidget.svelte';
	import { BookOpen, Library, Clock, Cpu, HardDrive, Network, TreePine, MessageCircle, Radar } from 'lucide-svelte';
	import { auth } from '$stores/auth.svelte';
	import { useDashboardStats } from '$lib/queries';
	import Skeleton from '$components/ui/Skeleton.svelte';

	const statsQuery = useDashboardStats();

	let stats = $derived({
		totalBooks: statsQuery.data?.total_books ?? 0,
		booksInProgress: statsQuery.data?.books_in_progress ?? 0,
		readingTimeTodayMins: statsQuery.data?.reading_time_today_mins ?? 0,
		tasksRunning: statsQuery.data?.tasks_running ?? 0,
		storageUsedGb: statsQuery.data?.storage_used_gb ?? 0,
		entitiesExtracted: statsQuery.data?.entities_extracted ?? 0,
	});

	let greeting = $derived(() => {
		const hour = new Date().getHours();
		if (hour < 6) return '夜深了';
		if (hour < 12) return '早上好';
		if (hour < 18) return '下午好';
		return '晚上好';
	});
</script>

<svelte:head>
	<title>Nova Reader — Dashboard</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 space-y-8 animate-fade-in">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">{greeting()}{auth.user ? `，${auth.user.display_name || auth.user.username}` : ''}</h1>
			<p class="mt-1 text-sm text-ink-400">你的个人数字文学智库</p>
		</div>
	</div>

	{#if statsQuery.isLoading}
		<!-- Dashboard skeleton -->
		<div class="grid grid-cols-2 gap-4 lg:grid-cols-3 xl:grid-cols-6">
			{#each Array(6) as _}
				<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4 space-y-3">
					<Skeleton class="h-4 w-1/2" />
					<Skeleton class="h-8 w-3/4" />
				</div>
			{/each}
		</div>
		<div class="grid gap-6 lg:grid-cols-3">
			<div class="lg:col-span-2 space-y-6">
				<Skeleton variant="card" class="h-48" />
				<Skeleton variant="card" class="h-48" />
			</div>
			<div class="space-y-6">
				<Skeleton variant="card" class="h-32" />
				<Skeleton variant="card" class="h-32" />
			</div>
		</div>
	{:else if stats.totalBooks === 0}
		<!-- Empty state for new users -->
		<div class="rounded-2xl border border-dashed border-ink-700/50 bg-ink-900/20 p-12 text-center">
			<div class="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-accent-500/10 ring-1 ring-accent-500/20">
				<BookOpen class="h-7 w-7 text-accent-400" strokeWidth={1.5} />
			</div>
			<h2 class="text-xl font-semibold text-ink-200 mb-2">开始你的阅读之旅</h2>
			<p class="text-sm text-ink-400 mb-6 max-w-md mx-auto">
				添加书库文件夹，Nova Reader 会自动整理、分析你的藏书
			</p>
			<div class="flex items-center justify-center">
				<a
					href="/libraries"
					class="inline-flex items-center gap-2 rounded-lg bg-accent-500 px-5 py-2.5 text-sm font-medium text-ink-950 hover:bg-accent-400 transition-colors"
				>
					添加书库
				</a>
			</div>
		</div>
	{:else}
		<!-- Stats Grid -->
		<div class="grid grid-cols-2 gap-4 lg:grid-cols-3 xl:grid-cols-6">
			<StatsCard
				label="书库总量"
				value={stats.totalBooks}
				icon={Library}
				trend={+12}
			/>
			<StatsCard
				label="正在阅读"
				value={stats.booksInProgress}
				icon={BookOpen}
			/>
			<StatsCard
				label="今日阅读"
				value="{stats.readingTimeTodayMins} 分钟"
				icon={Clock}
			/>
			<StatsCard
				label="后台任务"
				value={stats.tasksRunning}
				icon={Cpu}
				variant={stats.tasksRunning > 0 ? 'active' : 'default'}
			/>
			<StatsCard
				label="存储占用"
				value="{stats.storageUsedGb} GB"
				icon={HardDrive}
			/>
			<StatsCard
				label="知识实体"
				value={stats.entitiesExtracted}
				icon={Network}
			/>
		</div>

		<!-- Main Content Grid -->
		<div class="grid gap-6 lg:grid-cols-3">
			<!-- Recent Books (2/3 width) -->
			<div class="lg:col-span-2 space-y-6">
				<ContinueReading />
				<RecentlyFinished />
				<NewArrivals />
			</div>

			<!-- Activity & Tasks (1/3 width) -->
			<div class="space-y-6">
				<ReadingStreak />

				<!-- Knowledge Quick Access -->
				<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
					<h3 class="text-xs font-semibold uppercase tracking-wider text-ink-500 mb-3">知识工具</h3>
					<div class="grid grid-cols-3 gap-2">
						<a href="/ontology" class="flex flex-col items-center gap-1.5 p-3 rounded-lg hover:bg-ink-800/50 transition-colors group">
							<TreePine class="w-5 h-5 text-amber-400 group-hover:scale-110 transition-transform" />
							<span class="text-[10px] text-ink-400 group-hover:text-ink-200">设定花园</span>
						</a>
						<a href="/chat" class="flex flex-col items-center gap-1.5 p-3 rounded-lg hover:bg-ink-800/50 transition-colors group">
							<MessageCircle class="w-5 h-5 text-blue-400 group-hover:scale-110 transition-transform" />
							<span class="text-[10px] text-ink-400 group-hover:text-ink-200">AI 对话</span>
						</a>
						<a href="/semantic-tags" class="flex flex-col items-center gap-1.5 p-3 rounded-lg hover:bg-ink-800/50 transition-colors group">
							<Radar class="w-5 h-5 text-green-400 group-hover:scale-110 transition-transform" />
							<span class="text-[10px] text-ink-400 group-hover:text-ink-200">智能标签</span>
						</a>
					</div>
				</div>

				<SeriesWidget />
				<ReadingMemories />
				<Recommendations />
				<ReadingActivity />
				<ActivityFeed />
				<TaskQueue />
			</div>
		</div>
	{/if}
</div>
