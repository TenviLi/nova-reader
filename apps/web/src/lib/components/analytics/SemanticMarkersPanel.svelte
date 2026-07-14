<script lang="ts">
	import { api } from '$services/api';
	import type { SemanticProfile, TagMarker } from '$types/models';
	import { getErrorMessage } from '$lib/utils';
	import { BookOpen, Flame, RefreshCw, Sparkles } from 'lucide-svelte';
	import { toast } from 'svelte-sonner';

	let {
		bookId,
		currentChapterIndex = null,
		compact = false,
	} = $props<{
		bookId: string;
		currentChapterIndex?: number | null;
		compact?: boolean;
	}>();

	let markers = $state.raw<TagMarker[]>([]);
	let profiles = $state.raw<SemanticProfile[]>([]);
	let loading = $state(true);
	let computing = $state(false);

	let currentChapterMarkers = $derived.by(() => {
		if (currentChapterIndex === null) return [];
		return markers.filter((marker) => marker.chapter_index === currentChapterIndex);
	});

	let visibleMarkers = $derived.by(() => {
		const source = currentChapterMarkers.length > 0 ? currentChapterMarkers : markers;
		return source.slice(0, compact ? 5 : 12);
	});

	let profileById = $derived.by(() => new Map(profiles.map((profile) => [profile.id, profile])));

	let categoryEntries = $derived.by(() => {
		const counts = new Map<string, number>();
		for (const marker of markers) {
			const profile = profileById.get(marker.profile_id) ?? profileById.get(marker.tag_profile_id);
			const category = profile?.category ?? 'custom';
			counts.set(category, (counts.get(category) ?? 0) + 1);
		}
		return [...counts.entries()]
			.sort((a, b) => b[1] - a[1])
			.slice(0, compact ? 3 : 6);
	});

	let matchedProfileCount = $derived.by(() => {
		const ids = new Set(markers.map((marker) => marker.profile_id || marker.tag_profile_id).filter(Boolean));
		return ids.size;
	});

	$effect(() => {
		void bookId;
		load();
	});

	async function load() {
		loading = true;
		try {
			const [markerRows, profileRows] = await Promise.all([
				api.getBookMarkers(bookId),
				api.getSemanticProfiles().catch(() => []),
			]);
			markers = markerRows;
			profiles = profileRows;
		} catch {
			markers = [];
			profiles = [];
		} finally {
			loading = false;
		}
	}

	async function computeTags() {
		if (computing) return;
		computing = true;
		try {
			await api.computeBookTags(bookId);
			toast.success('智能标签计算任务已提交');
			await load();
		} catch (err) {
			toast.error(getErrorMessage(err) ?? '智能标签计算失败');
		} finally {
			computing = false;
		}
	}

	function categoryLabel(category: string): string {
		switch (category) {
			case 'trope': return '桥段';
			case 'emotion': return '情绪';
			case 'setting': return '设定';
			case 'warning': return '雷区';
			case 'custom': return '自定义';
			default: return category;
		}
	}

	function scoreLabel(score: number): string {
		return `${Math.round(Math.max(0, Math.min(1, score)) * 100)}%`;
	}

	function markerHref(marker: TagMarker): string {
		const params = new URLSearchParams({ chapter: String(marker.chapter_index) });
		if (marker.chunk_index !== undefined) {
			params.set('chunk', String(marker.chunk_index));
		} else if (marker.offset !== null && marker.offset !== undefined) {
			params.set('offset', String(marker.offset));
		}
		return `/reading/${bookId}?${params.toString()}`;
	}
</script>

<section class="rounded-xl border border-ink-800/60 bg-ink-900/30 p-4" aria-label="语义标记片段">
	<div class="flex items-start justify-between gap-3">
		<div class="min-w-0">
			<h4 class="flex items-center gap-2 text-sm font-medium text-ink-200">
				<Flame size={15} class="text-amber-300" />
				语义标记
			</h4>
			{#if markers.length > 0}
				<p class="mt-1 text-xs text-ink-500">
					{markers.length} 个命中片段 · {matchedProfileCount} 个画像
				</p>
			{/if}
		</div>
		<div class="flex shrink-0 items-center gap-2">
			{#if currentChapterIndex !== null}
				<span class="rounded-full bg-ink-950/60 px-2 py-0.5 text-[10px] text-ink-500">
					第 {currentChapterIndex + 1} 章
				</span>
			{/if}
			<button
				type="button"
				onclick={computeTags}
				disabled={computing}
				aria-label="计算本书智能标签"
				class="grid h-7 w-7 place-items-center rounded-md border border-ink-800/70 text-ink-500 transition-colors hover:border-accent-500/30 hover:text-accent-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 disabled:cursor-not-allowed disabled:opacity-50"
			>
				<RefreshCw size={13} class={computing ? 'animate-spin' : ''} aria-hidden="true" />
			</button>
		</div>
	</div>

	{#if categoryEntries.length > 0}
		<div class="mt-3 flex flex-wrap gap-1.5">
			{#each categoryEntries as [category, count]}
				<span class="rounded-full bg-ink-950/50 px-2 py-0.5 text-[10px] text-ink-400">
					{categoryLabel(category)} {count}
				</span>
			{/each}
		</div>
	{/if}

	{#if loading}
		<div class="mt-4 space-y-2">
			{#each Array(compact ? 2 : 4) as _}
				<div class="h-16 animate-pulse rounded-lg bg-ink-950/45"></div>
			{/each}
		</div>
	{:else if markers.length === 0}
		<div class="mt-4 rounded-lg border border-dashed border-ink-800/70 bg-ink-950/20 p-5 text-center">
			<Sparkles size={24} class="mx-auto mb-2 text-ink-600" />
			<p class="text-sm text-ink-400">暂无语义标记</p>
			<p class="mt-1 text-xs text-ink-600">计算智能标签后，会在这里显示高置信命中片段。</p>
			<button
				type="button"
				onclick={computeTags}
				disabled={computing}
				class="mt-4 inline-flex items-center gap-2 rounded-lg bg-accent-500 px-3 py-1.5 text-xs font-medium text-ink-950 transition-colors hover:bg-accent-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-300/80 disabled:cursor-not-allowed disabled:opacity-50"
			>
				<RefreshCw size={13} class={computing ? 'animate-spin' : ''} aria-hidden="true" />
				{computing ? '提交中…' : '计算智能标签'}
			</button>
		</div>
	{:else}
		{#if currentChapterIndex !== null && currentChapterMarkers.length === 0}
			<p class="mt-3 text-xs text-ink-600">当前章节暂无命中，显示全书高置信片段。</p>
		{/if}
		<div class="mt-3 space-y-2">
			{#each visibleMarkers as marker}
				{@const profile = profileById.get(marker.profile_id) ?? profileById.get(marker.tag_profile_id)}
				<a
					href={markerHref(marker)}
					class="block rounded-lg border border-ink-800/50 bg-ink-950/30 p-3 transition-colors hover:border-accent-500/30 hover:bg-ink-950/60 focus:outline-none focus:ring-2 focus:ring-accent-500/40"
				>
					<div class="mb-2 flex items-center gap-2">
						<BookOpen size={12} class="text-ink-500" />
						<span class="text-[11px] text-ink-500">第 {marker.chapter_index + 1} 章</span>
						{#if profile}
							<span class="inline-flex min-w-0 items-center gap-1 rounded-full bg-ink-900/70 px-2 py-0.5 text-[10px] text-ink-300">
								<span class="h-1.5 w-1.5 shrink-0 rounded-full" style="background-color: {profile.color}"></span>
								<span class="truncate">{profile.name}</span>
							</span>
						{/if}
						<span class="ml-auto rounded-full bg-accent-500/10 px-2 py-0.5 text-[10px] font-medium text-accent-300">
							{scoreLabel(marker.score)}
						</span>
					</div>
					<p class="line-clamp-3 text-xs leading-5 text-ink-300">{marker.snippet}</p>
				</a>
			{/each}
		</div>
	{/if}
</section>
