<script lang="ts">
	import { BookOpen, Hash, Sparkles, Network } from 'lucide-svelte';

	interface Props {
		bookTitle: string;
		chapterTitle?: string;
		chapterIndex?: number;
		content: string;
		highlighted?: string;
		score: number;
		source: 'keyword' | 'semantic' | 'graph';
		onClick?: () => void;
	}

	let {
		bookTitle,
		chapterTitle,
		chapterIndex,
		content,
		highlighted,
		score,
		source,
		onClick,
	}: Props = $props();

	const SOURCE_CONFIG = {
		keyword: { icon: Hash, label: '关键词', color: 'text-blue-500', bg: 'bg-blue-50 dark:bg-blue-900/20' },
		semantic: { icon: Sparkles, label: '语义', color: 'text-purple-500', bg: 'bg-purple-50 dark:bg-purple-900/20' },
		graph: { icon: Network, label: '图谱', color: 'text-emerald-500', bg: 'bg-emerald-50 dark:bg-emerald-900/20' },
	};

	let sourceInfo = $derived(SOURCE_CONFIG[source] ?? SOURCE_CONFIG.keyword);
	let SourceIcon = $derived(sourceInfo.icon);
	let scorePercent = $derived(Math.round(score * 100));
	let scoreColor = $derived(
		score >= 0.8 ? 'bg-green-500' :
		score >= 0.5 ? 'bg-amber-500' :
		'bg-ink-300'
	);
</script>

<button
	class="group w-full rounded-lg border border-ink-100 bg-white p-4 text-left transition-all hover:border-accent-200 hover:shadow-md dark:border-ink-700 dark:bg-ink-900 dark:hover:border-accent-700"
	onclick={onClick}
	type="button"
>
	<!-- Header: Book title + source badge -->
	<div class="mb-2 flex items-start justify-between gap-2">
		<div class="flex items-center gap-2 text-xs text-ink-500">
			<BookOpen class="h-3.5 w-3.5 flex-shrink-0" />
			<span class="line-clamp-1 font-medium text-ink-700 dark:text-ink-300">
				{bookTitle}
			</span>
			{#if chapterTitle}
				<span class="text-ink-400">·</span>
				<span class="line-clamp-1">{chapterTitle}</span>
			{/if}
		</div>
		<!-- Source badge -->
		<span class="flex flex-shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-xs {sourceInfo.bg} {sourceInfo.color}">
			<SourceIcon class="h-3 w-3" />
			{sourceInfo.label}
		</span>
	</div>

	<!-- Content snippet -->
	<div class="mb-3 text-sm leading-relaxed text-ink-600 dark:text-ink-400">
		{#if highlighted}
			{@html highlighted}
		{:else}
			<p class="line-clamp-3">{content}</p>
		{/if}
	</div>

	<!-- Footer: Score bar -->
	<div class="flex items-center gap-3">
		<!-- Score visualization bar -->
		<div class="flex flex-1 items-center gap-2">
			<div class="h-1.5 flex-1 overflow-hidden rounded-full bg-ink-100 dark:bg-ink-700">
				<div
					class="h-full rounded-full transition-all {scoreColor}"
					style="width: {scorePercent}%"
				></div>
			</div>
			<span class="text-xs font-mono text-ink-400">{scorePercent}%</span>
		</div>

		{#if chapterIndex !== undefined}
			<span class="text-xs text-ink-400">
				第{chapterIndex + 1}章
			</span>
		{/if}
	</div>
</button>
