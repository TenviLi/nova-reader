<script lang="ts">
	import { Bookmark, Sparkles, AlertTriangle, TrendingUp, Eye, Loader2 } from 'lucide-svelte';

	interface SmartBookmark {
		id: string;
		chapter_index: number;
		chapter_title: string;
		position: number; // character offset
		type: 'pivot' | 'foreshadowing' | 'revelation' | 'climax';
		description: string;
		confidence: number;
		related_chapter?: number;
	}

	interface Props {
		bookId: string;
		bookmarks?: SmartBookmark[];
		loading?: boolean;
		onAnalyze?: () => void;
		onBookmarkClick?: (bookmark: SmartBookmark) => void;
	}

	let { bookId, bookmarks = [], loading = false, onAnalyze, onBookmarkClick }: Props = $props();

	const TYPE_CONFIG = {
		pivot: { icon: TrendingUp, label: '转折点', color: 'text-red-500', bg: 'bg-red-50 dark:bg-red-900/20' },
		foreshadowing: { icon: Eye, label: '伏笔', color: 'text-purple-500', bg: 'bg-purple-50 dark:bg-purple-900/20' },
		revelation: { icon: Sparkles, label: '揭示', color: 'text-amber-500', bg: 'bg-amber-50 dark:bg-amber-900/20' },
		climax: { icon: AlertTriangle, label: '高潮', color: 'text-orange-500', bg: 'bg-orange-50 dark:bg-orange-900/20' },
	};

	let sortedBookmarks = $derived(
		[...bookmarks].sort((a, b) => a.chapter_index - b.chapter_index)
	);
</script>

<div class="rounded-xl border border-ink-100 bg-white p-5 dark:border-ink-700 dark:bg-ink-900">
	<div class="mb-4 flex items-center justify-between">
		<div class="flex items-center gap-2">
			<Bookmark class="h-5 w-5 text-accent-500" />
			<h3 class="text-lg font-semibold text-ink-800 dark:text-ink-200">智能书签</h3>
			{#if bookmarks.length > 0}
				<span class="rounded-full bg-ink-100 px-2 py-0.5 text-xs text-ink-500 dark:bg-ink-700">
					{bookmarks.length}
				</span>
			{/if}
		</div>
		{#if onAnalyze}
			<button
				class="rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-accent-600 disabled:opacity-50"
				onclick={onAnalyze}
				disabled={loading}
				type="button"
			>
				{#if loading}
					<Loader2 class="inline h-4 w-4 animate-spin" />
				{:else}
					自动标记
				{/if}
			</button>
		{/if}
	</div>

	{#if bookmarks.length === 0 && !loading}
		<div class="py-8 text-center text-ink-400">
			<Bookmark class="mx-auto mb-3 h-10 w-10 opacity-30" />
			<p class="text-sm">AI 将自动检测剧情转折、伏笔和高潮</p>
		</div>
	{:else}
		<!-- Type summary -->
		<div class="mb-4 flex flex-wrap gap-2">
			{#each Object.entries(TYPE_CONFIG) as [type, config]}
				{@const count = bookmarks.filter(b => b.type === type).length}
				{@const Icon = config.icon}
				{#if count > 0}
					<div class="flex items-center gap-1.5 rounded-full px-3 py-1 text-xs {config.bg} {config.color}">
						<Icon class="h-3 w-3" />
						<span>{config.label}</span>
						<span class="font-semibold">{count}</span>
					</div>
				{/if}
			{/each}
		</div>

		<!-- Timeline list -->
		<div class="relative space-y-0">
			<!-- Vertical line -->
			<div class="absolute left-[15px] top-2 bottom-2 w-px bg-ink-200 dark:bg-ink-700"></div>

			{#each sortedBookmarks as bookmark (bookmark.id)}
				{@const config = TYPE_CONFIG[bookmark.type]}
				{@const Icon = config.icon}
				<button
					class="group relative flex w-full items-start gap-3 rounded-lg p-2 text-left transition-colors hover:bg-ink-50 dark:hover:bg-ink-800"
					onclick={() => onBookmarkClick?.(bookmark)}
					type="button"
				>
					<!-- Dot on timeline -->
					<div class="relative z-10 mt-1 flex h-[30px] w-[30px] flex-shrink-0 items-center justify-center rounded-full border-2 border-white bg-white shadow-sm dark:border-ink-900 dark:bg-ink-900 {config.bg}">
						<Icon class="h-3.5 w-3.5 {config.color}" />
					</div>

					<!-- Content -->
					<div class="min-w-0 flex-1 pt-0.5">
						<div class="flex items-center gap-2">
							<span class="text-xs font-medium text-ink-500">第{bookmark.chapter_index + 1}章</span>
							{#if bookmark.chapter_title}
								<span class="truncate text-xs text-ink-400">{bookmark.chapter_title}</span>
							{/if}
						</div>
						<p class="mt-0.5 text-sm text-ink-700 dark:text-ink-300">
							{bookmark.description}
						</p>
						{#if bookmark.related_chapter !== undefined}
							<span class="mt-1 inline-block text-xs text-accent-500">
								→ 关联第{bookmark.related_chapter + 1}章
							</span>
						{/if}
					</div>

					<!-- Confidence -->
					<span class="flex-shrink-0 text-xs text-ink-300">
						{Math.round(bookmark.confidence * 100)}%
					</span>
				</button>
			{/each}
		</div>
	{/if}
</div>
