<script lang="ts">
	import { api } from '$services/api';
	import { Flame } from 'lucide-svelte';

	let { bookId } = $props<{ bookId: string }>();

	interface HeatmapRow {
		chapter_index: number;
		tag_profile_id: string;
		name: string;
		color: string;
		avg_score: number;
		max_score: number;
		match_count: number;
	}

	let data = $state<HeatmapRow[]>([]);
	let loading = $state(true);

	// Derive unique profiles and chapter range
	let profiles = $derived(
		[...new Map(data.map(d => [d.tag_profile_id, { id: d.tag_profile_id, name: d.name, color: d.color }])).values()]
	);
	let chapters = $derived(
		[...new Set(data.map(d => d.chapter_index))].sort((a, b) => a - b)
	);

	$effect(() => {
		loadHeatmap();
	});

	async function loadHeatmap() {
		loading = true;
		try {
			data = await api.getBookHeatmap(bookId);
		} catch { data = []; }
		finally { loading = false; }
	}

	function getCellScore(chapter: number, profileId: string): number {
		return data.find(d => d.chapter_index === chapter && d.tag_profile_id === profileId)?.avg_score ?? 0;
	}

	function cellOpacity(score: number): number {
		return Math.max(0.05, Math.min(1, score * 2.5));
	}
</script>

{#if loading}
	<div class="text-center py-8 text-ink-500">加载热力图...</div>
{:else if data.length === 0}
	<div class="text-center py-8 text-ink-500">
		<Flame size={32} class="mx-auto mb-2 opacity-30" />
		<p class="text-sm">暂无热力图数据</p>
		<p class="text-xs mt-1 text-ink-600">请先计算该书的智能标签</p>
	</div>
{:else}
	<div class="overflow-x-auto">
		<div class="min-w-[600px]">
			<!-- Header: profile labels -->
			<div class="flex items-end gap-0.5 mb-1 pl-14">
				{#each profiles as p}
					<div
						class="w-5 h-16 flex items-end justify-center"
						title={p.name}
					>
						<span class="text-[9px] text-ink-500 -rotate-45 origin-bottom-left whitespace-nowrap">{p.name}</span>
					</div>
				{/each}
			</div>

			<!-- Rows: one per chapter -->
			<div class="space-y-[1px]">
				{#each chapters as ch}
					<div class="flex items-center gap-0.5">
						<span class="w-12 text-right text-[10px] text-ink-600 tabular-nums pr-1.5">
							{ch + 1}
						</span>
						{#each profiles as p}
							{@const score = getCellScore(ch, p.id)}
							<div
								class="w-5 h-3.5 rounded-[2px] transition-opacity"
								style="background-color: {p.color}; opacity: {cellOpacity(score)}"
								title="{p.name} · 第{ch + 1}章 · {(score * 100).toFixed(1)}%"
							></div>
						{/each}
					</div>
				{/each}
			</div>

			<!-- Legend -->
			<div class="flex items-center gap-4 mt-3 pl-14">
				{#each profiles as p}
					<div class="flex items-center gap-1.5">
						<div class="w-3 h-3 rounded-sm" style="background-color: {p.color}"></div>
						<span class="text-[10px] text-ink-500">{p.name}</span>
					</div>
				{/each}
			</div>
		</div>
	</div>
{/if}
