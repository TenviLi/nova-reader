<script lang="ts">
	import { Tag, BookOpen, Search } from 'lucide-svelte';
	import { Input } from '$lib/components/ui/input';
	import { useBookTags } from '$lib/queries';

	interface TagItem {
		name: string;
		count: number;
	}

	const tagsQuery = useBookTags();

	let tags = $derived<TagItem[]>((tagsQuery.data ?? []).map(t => ({ name: t.tag, count: t.count })));
	let loading = $derived(tagsQuery.isLoading);
	let filter = $state('');

	let filteredTags = $derived(() => {
		if (!filter.trim()) return tags;
		const q = filter.toLowerCase();
		return tags.filter(t => t.name.toLowerCase().includes(q));
	});
</script>

<svelte:head>
	<title>Nova Reader — 标签</title>
</svelte:head>

<div class="mx-auto max-w-[1200px] px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">标签</h1>
			<p class="mt-1 text-sm text-ink-400">所有书籍标签及其计数</p>
		</div>
	</div>

	<div class="relative max-w-sm">
		<Search size={14} class="absolute left-3 top-1/2 -translate-y-1/2 text-ink-500" />
		<Input
			bind:value={filter}
			placeholder="搜索标签..."
			class="pl-9 bg-ink-900/50 border-ink-700/50"
		/>
	</div>

	{#if loading}
		<div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3">
			{#each Array(15) as _}
				<div class="h-16 rounded-lg bg-ink-900/50 animate-pulse"></div>
			{/each}
		</div>
	{:else if filteredTags().length === 0}
		<div class="text-center py-16">
			<Tag size={32} class="mx-auto text-ink-600 mb-3" />
			<p class="text-ink-400">{filter ? '没有匹配的标签' : '暂无标签'}</p>
			<p class="text-xs text-ink-500 mt-1">通过 AI 分析或手动为书籍添加标签后会在此展示</p>
		</div>
	{:else}
		<div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3">
			{#each filteredTags() as tag}
					<a
						href="/library?q={encodeURIComponent(tag.name)}"
						class="flex items-center gap-3 rounded-lg border border-ink-700/40 bg-ink-900/30 px-4 py-3 hover:bg-ink-800/50 hover:border-ink-600 transition-all group"
					>
					<Tag size={14} class="text-ink-500 group-hover:text-accent-400 transition-colors" />
					<div class="flex-1 min-w-0">
						<p class="text-sm text-ink-200 truncate group-hover:text-accent-400 transition-colors">{tag.name}</p>
						<p class="text-xs text-ink-500">{tag.count} 本</p>
					</div>
				</a>
			{/each}
		</div>
	{/if}
</div>
