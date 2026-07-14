<script lang="ts">
	import ReadingProgress from '$components/ReadingProgress.svelte';
	import type { Book } from '$types/models';
	import { formatDuration, timeAgo } from '$utils/format';
	import { useReadingBooks } from '$lib/queries';
	import Skeleton from '$components/ui/Skeleton.svelte';

	interface ReadingItem {
		book: Book;
		progress: number;
		last_read_at: string;
		current_chapter: string;
	}

	const readingQuery = useReadingBooks(() => ({ reading_status: 'reading', sort_by: 'last_read_at', per_page: 10 }));
	const finishedQuery = useReadingBooks(() => ({ reading_status: 'completed', sort_by: 'updated_at', per_page: 5 }));

	let currentlyReading = $derived<ReadingItem[]>(
		(readingQuery.data?.data ?? []).map(b => ({
			book: b,
			progress: b.progress ?? 0,
			last_read_at: b.updated_at,
			current_chapter: '',
		}))
	);
	let recentlyFinished = $derived<ReadingItem[]>(
		(finishedQuery.data?.data ?? []).map(b => ({
			book: b,
			progress: 1,
			last_read_at: b.updated_at,
			current_chapter: '',
		}))
	);
	let loading = $derived(readingQuery.isLoading || finishedQuery.isLoading);
	let error = $derived(readingQuery.error ? readingQuery.error.message : null);
</script>

<svelte:head>
	<title>Nova Reader — 阅读</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 space-y-8 animate-fade-in">
	<div>
		<h1 class="text-2xl font-bold text-ink-50">阅读</h1>
		<p class="mt-1 text-ink-400">继续你的阅读之旅</p>
	</div>

	<!-- Currently Reading -->
	<section>
		<h2 class="text-sm font-medium text-ink-400 uppercase tracking-wider mb-4">正在阅读</h2>
		{#if error}
			<div class="text-center py-8 border border-red-900/50 rounded-xl bg-red-950/20">
				<p class="text-red-400 text-sm">{error}</p>
				<button onclick={() => location.reload()} class="mt-2 text-xs text-ink-400 hover:text-ink-200">重试</button>
			</div>
		{:else if loading}
			<div class="grid grid-cols-1 md:grid-cols-2 gap-4">
				{#each Array(4) as _}
					<div class="flex gap-4 p-4 bg-ink-900/30 border border-ink-800/50 rounded-xl">
						<Skeleton class="w-16 h-24 shrink-0 rounded-lg" />
						<div class="flex-1 space-y-3">
							<Skeleton class="h-5 w-3/4" />
							<Skeleton class="h-3 w-1/3" />
							<Skeleton class="h-1.5 w-full rounded-full" />
							<Skeleton class="h-3 w-1/4" />
						</div>
					</div>
				{/each}
			</div>
		{:else if currentlyReading.length === 0}
			<div class="text-center py-12 border border-ink-800/50 rounded-xl">
				<p class="text-ink-500 mb-2">还没有正在阅读的书</p>
				<a href="/library" class="text-accent-400 hover:text-accent-300 text-sm">去书库看看 →</a>
			</div>
		{:else}
			<div class="grid grid-cols-1 md:grid-cols-2 gap-4">
				{#each currentlyReading as item (item.book.id)}
					<a
						href="/reading/{item.book.id}"
						class="group flex gap-4 p-4 bg-ink-900/30 border border-ink-800/50 rounded-xl hover:border-accent-500/30 hover:bg-ink-800/30 transition-all"
					>
						<!-- Cover -->
						<div class="w-16 h-24 shrink-0 rounded-lg overflow-hidden bg-ink-800">
							{#if item.book.cover_path}
								<img src="/api/covers/{item.book.cover_path}" alt={item.book.title} class="w-full h-full object-cover" />
							{:else}
								<div class="w-full h-full flex items-center justify-center text-2xl">📖</div>
							{/if}
						</div>

						<!-- Info -->
						<div class="flex-1 min-w-0 flex flex-col justify-between">
							<div>
								<h3 class="font-medium text-ink-100 truncate group-hover:text-accent-400 transition-colors">
									{item.book.title}
								</h3>
								<p class="text-xs text-ink-500 mt-0.5">{item.book.author}</p>
							</div>

							<div class="flex items-center gap-3">
								<div class="flex-1">
									<div class="h-1.5 bg-ink-800 rounded-full overflow-hidden">
										<div
											class="h-full bg-accent-500 rounded-full transition-all duration-500"
											style="width: {item.progress * 100}%"
										></div>
									</div>
								</div>
								<span class="text-xs text-ink-400 shrink-0">
									{Math.round(item.progress * 100)}%
								</span>
							</div>

							<p class="text-xs text-ink-500">{timeAgo(item.last_read_at)}</p>
						</div>
					</a>
				{/each}
			</div>
		{/if}
	</section>

	<!-- Recently Finished -->
	{#if recentlyFinished.length > 0}
		<section>
			<h2 class="text-sm font-medium text-ink-400 uppercase tracking-wider mb-4">最近读完</h2>
			<div class="grid grid-cols-2 md:grid-cols-5 gap-3">
				{#each recentlyFinished as item (item.book.id)}
					<a href="/library/{item.book.id}" class="group text-center">
						<div class="aspect-[3/4] bg-ink-800 rounded-lg overflow-hidden mb-2 relative">
							{#if item.book.cover_path}
								<img src="/api/covers/{item.book.cover_path}" alt={item.book.title} class="w-full h-full object-cover" />
							{:else}
								<div class="w-full h-full flex items-center justify-center text-3xl">📖</div>
							{/if}
							<div class="absolute inset-0 bg-gradient-to-t from-ink-950/60 to-transparent"></div>
							<div class="absolute bottom-2 left-2">
								<span class="px-1.5 py-0.5 bg-emerald-500/20 text-emerald-400 text-[10px] rounded font-medium">已读完</span>
							</div>
						</div>
						<p class="text-xs text-ink-300 truncate group-hover:text-accent-400 transition-colors">{item.book.title}</p>
					</a>
				{/each}
			</div>
		</section>
	{/if}

	<!-- Reading Stats Summary -->
	<section class="p-6 bg-ink-900/30 border border-ink-800/50 rounded-xl">
		<h2 class="text-sm font-medium text-ink-400 uppercase tracking-wider mb-4">阅读统计</h2>
		<div class="grid grid-cols-4 gap-4 text-center">
			<div>
				<div class="text-2xl font-bold text-ink-100">{currentlyReading.length}</div>
				<div class="text-xs text-ink-500 mt-1">正在阅读</div>
			</div>
			<div>
				<div class="text-2xl font-bold text-ink-100">{recentlyFinished.length}</div>
				<div class="text-xs text-ink-500 mt-1">已完成</div>
			</div>
			<div>
				<div class="text-2xl font-bold text-ink-100">0</div>
				<div class="text-xs text-ink-500 mt-1">今日阅读(分钟)</div>
			</div>
			<div>
				<div class="text-2xl font-bold text-ink-100">0</div>
				<div class="text-xs text-ink-500 mt-1">本周标注</div>
			</div>
		</div>
	</section>
</div>
