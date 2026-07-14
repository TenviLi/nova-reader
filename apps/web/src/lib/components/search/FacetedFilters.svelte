<script lang="ts">
	import { ChevronDown, ChevronRight } from 'lucide-svelte';
	import type { BookFormat, Language, EntityType } from '$lib/types/models';

	interface FacetGroup {
		label: string;
		key: string;
		expanded: boolean;
		options: FacetOption[];
	}

	interface FacetOption {
		value: string;
		label: string;
		count?: number;
	}

	interface Props {
		onFilterChange?: (filters: Record<string, string[]>) => void;
		resultCounts?: Record<string, Record<string, number>>;
	}

	let { onFilterChange, resultCounts = {} }: Props = $props();

	let selected = $state<Record<string, string[]>>({});

	let facetGroups = $state<FacetGroup[]>([
		{
			label: '语言',
			key: 'language',
			expanded: true,
			options: [
				{ value: 'zh', label: '中文' },
				{ value: 'en', label: 'English' },
				{ value: 'ja', label: '日本語' },
				{ value: 'ko', label: '한국어' },
			],
		},
		{
			label: '格式',
			key: 'format',
			expanded: false,
			options: [
				{ value: 'txt', label: 'TXT' },
				{ value: 'epub', label: 'EPUB' },
				{ value: 'pdf', label: 'PDF' },
				{ value: 'docx', label: 'DOCX' },
				{ value: 'doc', label: 'DOC' },
				{ value: 'md', label: 'Markdown' },
				{ value: 'html', label: 'HTML' },
			],
		},
		{
			label: '实体类型',
			key: 'entity_type',
			expanded: false,
			options: [
				{ value: 'character', label: '人物' },
				{ value: 'location', label: '地点' },
				{ value: 'organization', label: '组织' },
				{ value: 'item', label: '物品' },
				{ value: 'skill', label: '技能' },
				{ value: 'event', label: '事件' },
				{ value: 'concept', label: '概念' },
			],
		},
		{
			label: '搜索来源',
			key: 'source',
			expanded: true,
			options: [
				{ value: 'keyword', label: '关键词匹配' },
				{ value: 'semantic', label: '语义检索' },
				{ value: 'graph', label: '知识图谱' },
			],
		},
		{
			label: '日期范围',
			key: 'date_range',
			expanded: false,
			options: [
				{ value: 'today', label: '今天' },
				{ value: 'week', label: '最近一周' },
				{ value: 'month', label: '最近一月' },
				{ value: 'year', label: '最近一年' },
			],
		},
	]);

	function toggleFacet(groupKey: string, value: string) {
		const current = selected[groupKey] ?? [];
		if (current.includes(value)) {
			selected[groupKey] = current.filter(v => v !== value);
		} else {
			selected[groupKey] = [...current, value];
		}
		selected = { ...selected };
		onFilterChange?.(selected);
	}

	function isSelected(groupKey: string, value: string): boolean {
		return (selected[groupKey] ?? []).includes(value);
	}

	function clearAll() {
		selected = {};
		onFilterChange?.({});
	}

	function getCount(groupKey: string, value: string): number | undefined {
		return resultCounts[groupKey]?.[value];
	}

	let hasActiveFilters = $derived(
		Object.values(selected).some(arr => arr.length > 0)
	);
</script>

<aside class="w-56 space-y-1 overflow-y-auto">
	<!-- Header -->
	<div class="flex items-center justify-between px-2 py-1.5">
		<span class="text-xs font-semibold uppercase tracking-wider text-ink-500">筛选条件</span>
		{#if hasActiveFilters}
			<button
				class="text-xs text-accent-500 hover:text-accent-600"
				onclick={clearAll}
			>
				清除全部
			</button>
		{/if}
	</div>

	<!-- Facet groups -->
	{#each facetGroups as group}
		<div class="border-t border-ink-100 dark:border-ink-700">
			<button
				class="flex w-full items-center justify-between px-2 py-2 text-left text-sm font-medium text-ink-700 hover:text-ink-900 dark:text-ink-300 dark:hover:text-ink-100"
				onclick={() => group.expanded = !group.expanded}
			>
				<span>{group.label}</span>
				{#if group.expanded}
					<ChevronDown class="h-3.5 w-3.5" />
				{:else}
					<ChevronRight class="h-3.5 w-3.5" />
				{/if}
			</button>

			{#if group.expanded}
				<div class="space-y-0.5 px-2 pb-2">
					{#each group.options as option}
						{@const count = getCount(group.key, option.value)}
						<label class="flex cursor-pointer items-center gap-2 rounded px-1.5 py-1 text-xs hover:bg-ink-50 dark:hover:bg-ink-800">
							<input
								type="checkbox"
								checked={isSelected(group.key, option.value)}
								onchange={() => toggleFacet(group.key, option.value)}
								class="h-3 w-3 rounded accent-accent-500"
							/>
							<span class="flex-1 text-ink-700 dark:text-ink-300">{option.label}</span>
							{#if count !== undefined}
								<span class="text-ink-400">{count}</span>
							{/if}
						</label>
					{/each}
				</div>
			{/if}
		</div>
	{/each}
</aside>
