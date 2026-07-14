<script lang="ts">
	import { Users, TrendingUp, Heart, Swords, ChevronDown, ChevronUp } from 'lucide-svelte';

	interface CharacterNode {
		name: string;
		first_appearance: number;
		last_appearance: number;
		importance_score: number;
	}

	interface RelationshipEdge {
		source: string;
		target: string;
		type: 'ally' | 'rival' | 'romantic' | 'family' | 'mentor';
		strength: number; // 0-1
		first_chapter: number;
		evolution: Array<{ chapter: number; strength: number; event?: string }>;
	}

	interface Props {
		bookId: string;
		characters?: CharacterNode[];
		relationships?: RelationshipEdge[];
		totalChapters?: number;
		onAnalyze?: () => void;
		loading?: boolean;
	}

	let {
		bookId,
		characters = [],
		relationships = [],
		totalChapters = 100,
		onAnalyze,
		loading = false,
	}: Props = $props();

	let selectedCharacter = $state<string | null>(null);
	let showAllCharacters = $state(false);

	const RELATION_COLORS = {
		ally: '#3b82f6',
		rival: '#ef4444',
		romantic: '#ec4899',
		family: '#8b5cf6',
		mentor: '#f59e0b',
	};

	const RELATION_LABELS = {
		ally: '盟友',
		rival: '对手',
		romantic: '恋人',
		family: '亲属',
		mentor: '师徒',
	};

	let sortedCharacters = $derived(
		[...characters].sort((a, b) => b.importance_score - a.importance_score)
	);

	let visibleCharacters = $derived(
		showAllCharacters ? sortedCharacters : sortedCharacters.slice(0, 8)
	);

	let selectedRelationships = $derived(
		selectedCharacter
			? relationships.filter(r => r.source === selectedCharacter || r.target === selectedCharacter)
			: relationships.slice(0, 10)
	);

	function getChapterX(chapter: number): number {
		return (chapter / totalChapters) * 100;
	}

	function getStrengthY(strength: number): number {
		return 100 - strength * 80 - 10;
	}

	function buildPath(evolution: Array<{ chapter: number; strength: number }>): string {
		if (evolution.length === 0) return '';
		const points = evolution.map(e => `${getChapterX(e.chapter)},${getStrengthY(e.strength)}`);
		return `M ${points.join(' L ')}`;
	}
</script>

<div class="rounded-xl border border-ink-100 bg-white p-6 dark:border-ink-700 dark:bg-ink-900">
	<!-- Header -->
	<div class="mb-4 flex items-center justify-between">
		<div class="flex items-center gap-2">
			<Users class="h-5 w-5 text-accent-500" />
			<h3 class="text-lg font-semibold text-ink-800 dark:text-ink-200">角色关系演化</h3>
		</div>
		{#if onAnalyze}
			<button
				class="rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-accent-600 disabled:opacity-50"
				onclick={onAnalyze}
				disabled={loading}
			>
				{loading ? '分析中...' : '开始分析'}
			</button>
		{/if}
	</div>

	{#if characters.length === 0}
		<div class="py-12 text-center text-ink-400">
			<Users class="mx-auto mb-3 h-12 w-12 opacity-30" />
			<p>点击「开始分析」提取角色关系演化</p>
		</div>
	{:else}
		<!-- Character list -->
		<div class="mb-6">
			<h4 class="mb-2 text-sm font-medium text-ink-600 dark:text-ink-400">主要角色</h4>
			<div class="flex flex-wrap gap-2">
				{#each visibleCharacters as char}
					<button
						class="rounded-full border px-3 py-1 text-sm transition-all {
							selectedCharacter === char.name
								? 'border-accent-500 bg-accent-50 text-accent-700 dark:bg-accent-900/20 dark:text-accent-300'
								: 'border-ink-200 text-ink-600 hover:border-accent-300 dark:border-ink-600 dark:text-ink-400'
						}"
						onclick={() => selectedCharacter = selectedCharacter === char.name ? null : char.name}
					>
						{char.name}
						<span class="ml-1 text-xs opacity-50">Ch.{char.first_appearance}-{char.last_appearance}</span>
					</button>
				{/each}
			</div>
			{#if sortedCharacters.length > 8}
				<button
					class="mt-2 flex items-center gap-1 text-xs text-accent-500 hover:text-accent-600"
					onclick={() => showAllCharacters = !showAllCharacters}
				>
					{#if showAllCharacters}
						<ChevronUp class="h-3 w-3" /> 收起
					{:else}
						<ChevronDown class="h-3 w-3" /> 显示全部 ({sortedCharacters.length})
					{/if}
				</button>
			{/if}
		</div>

		<!-- Relationship Timeline SVG -->
		<div class="mb-4">
			<h4 class="mb-2 text-sm font-medium text-ink-600 dark:text-ink-400">
				关系强度变化
				{#if selectedCharacter}
					<span class="text-accent-500">— {selectedCharacter}</span>
				{/if}
			</h4>
			<div class="relative w-full overflow-hidden rounded-lg border border-ink-100 bg-ink-50 dark:border-ink-700 dark:bg-ink-800">
				<svg viewBox="0 0 100 100" class="h-48 w-full" preserveAspectRatio="none">
					<!-- Grid lines -->
					{#each [0.25, 0.5, 0.75] as y}
						<line
							x1="0" y1={getStrengthY(y)}
							x2="100" y2={getStrengthY(y)}
							stroke="currentColor" stroke-dasharray="1,2"
							class="text-ink-200 dark:text-ink-600" stroke-width="0.3"
						/>
					{/each}

					<!-- Relationship curves -->
					{#each selectedRelationships as rel}
						{#if rel.evolution.length > 1}
							<path
								d={buildPath(rel.evolution)}
								fill="none"
								stroke={RELATION_COLORS[rel.type]}
								stroke-width="1.5"
								stroke-linecap="round"
								stroke-linejoin="round"
								opacity="0.8"
							/>
							<!-- Points -->
							{#each rel.evolution as point}
								<circle
									cx={getChapterX(point.chapter)}
									cy={getStrengthY(point.strength)}
									r="1.5"
									fill={RELATION_COLORS[rel.type]}
								>
									{#if point.event}
										<title>{point.event} (Ch.{point.chapter})</title>
									{/if}
								</circle>
							{/each}
						{/if}
					{/each}
				</svg>

				<!-- X-axis labels -->
				<div class="flex justify-between px-2 py-1 text-xs text-ink-400">
					<span>第1章</span>
					<span>第{Math.floor(totalChapters / 2)}章</span>
					<span>第{totalChapters}章</span>
				</div>
			</div>
		</div>

		<!-- Legend -->
		<div class="flex flex-wrap gap-3">
			{#each Object.entries(RELATION_COLORS) as [type, color]}
				<div class="flex items-center gap-1.5 text-xs text-ink-500">
					<div class="h-2 w-4 rounded-full" style="background-color: {color}"></div>
					<span>{RELATION_LABELS[type as keyof typeof RELATION_LABELS]}</span>
				</div>
			{/each}
		</div>

		<!-- Relationship detail cards -->
		{#if selectedRelationships.length > 0}
			<div class="mt-4 space-y-2">
				<h4 class="text-sm font-medium text-ink-600 dark:text-ink-400">关系详情</h4>
				{#each selectedRelationships.slice(0, 5) as rel}
					<div class="flex items-center gap-3 rounded-lg border border-ink-100 bg-ink-50/50 p-3 dark:border-ink-700 dark:bg-ink-800/50">
						<div class="h-2 w-2 rounded-full" style="background-color: {RELATION_COLORS[rel.type]}"></div>
						<span class="text-sm font-medium text-ink-700 dark:text-ink-300">{rel.source}</span>
						<span class="text-xs text-ink-400">—{RELATION_LABELS[rel.type]}—</span>
						<span class="text-sm font-medium text-ink-700 dark:text-ink-300">{rel.target}</span>
						<span class="ml-auto text-xs text-ink-400">
							强度 {Math.round(rel.strength * 100)}%
						</span>
					</div>
				{/each}
			</div>
		{/if}
	{/if}
</div>
