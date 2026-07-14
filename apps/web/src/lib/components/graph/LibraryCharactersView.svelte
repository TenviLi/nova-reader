<script lang="ts">
	import { api } from '$services/api';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import { Users, Search, BookOpen } from 'lucide-svelte';
	import { Input } from '$lib/components/ui/input';

	let { libraryId, seriesId } = $props<{ libraryId?: string; seriesId?: string }>();

	let loading = $state(true);
	let searchQuery = $state('');
	let filterType = $state<string>('character');
	let entities = $state<Array<{
		id: string;
		name: string;
		type: string;
		description: string;
		mention_count: number;
		book_count: number;
	}>>([]);

	const typeFilters = [
		{ value: 'character', label: '人物' },
		{ value: 'location', label: '地点' },
		{ value: 'organization', label: '组织' },
		{ value: 'item', label: '物品' },
		{ value: 'event', label: '事件' },
	];

	const typeColors: Record<string, string> = {
		character: 'bg-amber-500/15 text-amber-400 ring-amber-500/20',
		location: 'bg-emerald-500/15 text-emerald-400 ring-emerald-500/20',
		organization: 'bg-indigo-500/15 text-indigo-400 ring-indigo-500/20',
		item: 'bg-pink-500/15 text-pink-400 ring-pink-500/20',
		event: 'bg-cyan-500/15 text-cyan-400 ring-cyan-500/20',
	};

	async function loadEntities() {
		loading = true;
		try {
			const list = await api.getEntities({
				type: filterType,
				library_id: libraryId || undefined,
				series_id: seriesId || undefined,
				search: searchQuery || undefined,
			});
			entities = list.map((e) => ({
				id: e.id,
				name: e.name,
				type: e.entity_type ?? filterType,
				description: e.description ?? '',
				mention_count: e.mention_count ?? 0,
				book_count: e.book_count ?? 0,
			}));
		} catch {
			entities = [];
		} finally {
			loading = false;
		}
	}

	function getInitials(name: string): string {
		return name.slice(0, 2);
	}

	function getAvatarColor(name: string): string {
		const colors = ['bg-amber-500/20', 'bg-emerald-500/20', 'bg-indigo-500/20', 'bg-rose-500/20', 'bg-cyan-500/20', 'bg-violet-500/20'];
		let hash = 0;
		for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash);
		return colors[Math.abs(hash) % colors.length];
	}

	onMount(loadEntities);

	$effect(() => {
		filterType; searchQuery;
		loadEntities();
	});
</script>

<div class="space-y-4">
	<!-- Filters -->
	<div class="flex items-center gap-3 flex-wrap">
		<div class="flex gap-1 rounded-lg border border-ink-700/50 bg-ink-900/50 p-0.5">
			{#each typeFilters as t}
				<button
					onclick={() => filterType = t.value}
					class="px-3 py-1 rounded text-xs font-medium transition-colors {filterType === t.value ? 'bg-accent-500/15 text-accent-400' : 'text-ink-500 hover:text-ink-300'}"
				>{t.label}</button>
			{/each}
		</div>
		<Input
			bind:value={searchQuery}
			placeholder="搜索角色名..."
			class="max-w-[200px] h-8 bg-ink-800/50 border-ink-700/60 text-ink-100 placeholder:text-ink-600 text-sm"
		/>
		<span class="text-xs text-ink-500">{entities.length} 个结果</span>
	</div>

	<!-- Entity grid -->
	{#if loading}
		<div class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-3">
			{#each Array(6) as _}
				<div class="h-20 rounded-xl bg-ink-900/50 animate-pulse"></div>
			{/each}
		</div>
	{:else if entities.length === 0}
		<div class="text-center py-12">
			<Users size={36} class="mx-auto text-ink-600 mb-2" />
			<p class="text-ink-400">暂无角色数据</p>
			<p class="text-xs text-ink-500 mt-1">对当前范围内的书籍执行 AI 实体提取后将显示角色列表</p>
		</div>
	{:else}
		<div class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-3">
			{#each entities as entity}
				<button
					onclick={() => goto(`/characters/${entity.id}`)}
					class="flex items-center gap-3 rounded-xl border border-ink-800/50 bg-ink-900/50 p-4 text-left hover:border-accent-500/30 hover:bg-ink-800/40 transition-all group"
				>
					<!-- Avatar -->
					<div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full {getAvatarColor(entity.name)} ring-1 ring-ink-700/30">
						<span class="text-sm font-semibold text-ink-200">{getInitials(entity.name)}</span>
					</div>
					<div class="min-w-0 flex-1">
						<p class="text-sm font-medium text-ink-100 truncate group-hover:text-accent-400 transition-colors">{entity.name}</p>
						<p class="text-xs text-ink-500 truncate">{entity.description || `出现 ${entity.mention_count} 次`}</p>
					</div>
					{#if entity.book_count > 0}
						<span class="flex items-center gap-1 text-[10px] text-ink-600">
							<BookOpen size={10} />
							{entity.book_count}
						</span>
					{/if}
				</button>
			{/each}
		</div>
	{/if}
</div>
