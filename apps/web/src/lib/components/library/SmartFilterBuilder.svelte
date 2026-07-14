<script lang="ts">
	import { X, Plus, Filter, Save } from 'lucide-svelte';
	import type { FilterCondition, FilterConjunction, FilterOperator, SmartFilter } from '$lib/types/models';

	interface Props {
		onApply?: (filter: Omit<SmartFilter, 'id' | 'created_at'>) => void;
		savedFilters?: SmartFilter[];
		onSave?: (filter: Omit<SmartFilter, 'id' | 'created_at'>) => void;
		onLoad?: (filter: SmartFilter) => void;
	}

	let { onApply, savedFilters = [], onSave, onLoad }: Props = $props();

	let conjunction = $state<FilterConjunction>('and');
	let conditions = $state<FilterCondition[]>([
		{ field: 'reading_status', operator: 'eq', value: '' }
	]);
	let filterName = $state('');
	let showSaveDialog = $state(false);

	const FIELDS = [
		{ value: 'reading_status', label: '阅读状态', type: 'select', options: ['unread', 'reading', 'completed', 'on_hold', 'dropped'] },
		{ value: 'status', label: '处理状态', type: 'select', options: ['pending', 'processing', 'ready', 'failed'] },
		{ value: 'language', label: '语言', type: 'select', options: ['zh', 'en', 'ja', 'ko'] },
		{ value: 'format', label: '格式', type: 'select', options: ['txt', 'epub', 'pdf', 'docx', 'doc', 'md', 'html'] },
		{ value: 'word_count', label: '字数', type: 'number' },
		{ value: 'rating', label: '评分', type: 'number' },
		{ value: 'tags', label: '标签', type: 'text' },
		{ value: 'author', label: '作者', type: 'text' },
		{ value: 'title', label: '书名', type: 'text' },
		{ value: 'series', label: '系列', type: 'text' },
		{ value: 'chapter_count', label: '章节数', type: 'number' },
		{ value: 'created_at', label: '添加日期', type: 'date' },
	] as const;

	const OPERATORS: Record<string, { value: FilterOperator; label: string }[]> = {
		select: [
			{ value: 'eq', label: '等于' },
			{ value: 'neq', label: '不等于' },
			{ value: 'in', label: '属于' },
			{ value: 'not_in', label: '不属于' },
		],
		number: [
			{ value: 'eq', label: '等于' },
			{ value: 'gt', label: '大于' },
			{ value: 'gte', label: '大于等于' },
			{ value: 'lt', label: '小于' },
			{ value: 'lte', label: '小于等于' },
		],
		text: [
			{ value: 'contains', label: '包含' },
			{ value: 'not_contains', label: '不包含' },
			{ value: 'eq', label: '等于' },
		],
		date: [
			{ value: 'gt', label: '晚于' },
			{ value: 'lt', label: '早于' },
		],
	};

	function getFieldType(fieldValue: string): string {
		return FIELDS.find(f => f.value === fieldValue)?.type ?? 'text';
	}

	function getFieldOptions(fieldValue: string): string[] {
		const field = FIELDS.find(f => f.value === fieldValue);
		return (field && 'options' in field) ? field.options as unknown as string[] : [];
	}

	function addCondition() {
		conditions = [...conditions, { field: 'reading_status', operator: 'eq', value: '' }];
	}

	function removeCondition(index: number) {
		conditions = conditions.filter((_, i) => i !== index);
	}

	function apply() {
		const validConditions = conditions.filter(c => c.value !== '' && c.value !== undefined);
		if (validConditions.length === 0) return;
		onApply?.({
			name: filterName || '未命名筛选',
			conjunction,
			conditions: validConditions,
		});
	}

	function save() {
		if (!filterName.trim()) return;
		onSave?.({
			name: filterName,
			conjunction,
			conditions: conditions.filter(c => c.value !== ''),
		});
		showSaveDialog = false;
	}

	function loadFilter(filter: SmartFilter) {
		conjunction = filter.conjunction;
		conditions = [...filter.conditions];
		filterName = filter.name;
		onLoad?.(filter);
	}

	const STATUS_LABELS: Record<string, string> = {
		unread: '未读', reading: '在读', completed: '已读', on_hold: '搁置', dropped: '弃读',
		pending: '待处理', processing: '处理中', ready: '就绪', failed: '失败',
		zh: '中文', en: '英语', ja: '日语', ko: '韩语',
		txt: 'TXT', epub: 'EPUB', pdf: 'PDF', docx: 'DOCX', doc: 'DOC', md: 'Markdown', html: 'HTML',
	};
</script>

<div class="space-y-3 rounded-lg border border-ink-200 bg-parchment-50 p-4 dark:border-ink-700 dark:bg-ink-900">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<div class="flex items-center gap-2 text-sm font-medium text-ink-700 dark:text-ink-300">
			<Filter class="h-4 w-4" />
			<span>智能筛选</span>
		</div>
		<div class="flex items-center gap-2">
			{#if savedFilters.length > 0}
				<select
					class="rounded border border-ink-200 bg-white px-2 py-1 text-xs dark:border-ink-600 dark:bg-ink-800"
					onchange={(e) => {
						const target = e.target as HTMLSelectElement;
						const filter = savedFilters.find(f => f.id === target.value);
						if (filter) loadFilter(filter);
					}}
				>
					<option value="">加载已保存...</option>
					{#each savedFilters as filter}
						<option value={filter.id}>{filter.name}</option>
					{/each}
				</select>
			{/if}
		</div>
	</div>

	<!-- Conjunction toggle -->
	<div class="flex items-center gap-2 text-xs">
		<span class="text-ink-500">满足</span>
		<button
			class="rounded px-2 py-0.5 font-medium transition-colors {conjunction === 'and' ? 'bg-accent-500 text-white' : 'bg-ink-100 text-ink-600 dark:bg-ink-700 dark:text-ink-300'}"
			onclick={() => conjunction = 'and'}
		>
			全部
		</button>
		<button
			class="rounded px-2 py-0.5 font-medium transition-colors {conjunction === 'or' ? 'bg-accent-500 text-white' : 'bg-ink-100 text-ink-600 dark:bg-ink-700 dark:text-ink-300'}"
			onclick={() => conjunction = 'or'}
		>
			任一
		</button>
		<span class="text-ink-500">条件</span>
	</div>

	<!-- Conditions -->
	<div class="space-y-2">
		{#each conditions as condition, index}
			<div class="flex items-center gap-2">
				<!-- Field selector -->
				<select
					class="w-24 rounded border border-ink-200 bg-white px-2 py-1.5 text-xs dark:border-ink-600 dark:bg-ink-800"
					bind:value={condition.field}
					onchange={() => { condition.operator = 'eq'; condition.value = ''; }}
				>
					{#each FIELDS as field}
						<option value={field.value}>{field.label}</option>
					{/each}
				</select>

				<!-- Operator -->
				<select
					class="w-20 rounded border border-ink-200 bg-white px-2 py-1.5 text-xs dark:border-ink-600 dark:bg-ink-800"
					bind:value={condition.operator}
				>
					{#each OPERATORS[getFieldType(condition.field)] ?? OPERATORS.text as op}
						<option value={op.value}>{op.label}</option>
					{/each}
				</select>

				<!-- Value input -->
				{#if getFieldType(condition.field) === 'select'}
					<select
						class="flex-1 rounded border border-ink-200 bg-white px-2 py-1.5 text-xs dark:border-ink-600 dark:bg-ink-800"
						bind:value={condition.value}
					>
						<option value="">选择...</option>
						{#each getFieldOptions(condition.field) as opt}
							<option value={opt}>{STATUS_LABELS[opt] ?? opt}</option>
						{/each}
					</select>
				{:else if getFieldType(condition.field) === 'number'}
					<input
						type="number"
						class="flex-1 rounded border border-ink-200 bg-white px-2 py-1.5 text-xs dark:border-ink-600 dark:bg-ink-800"
						bind:value={condition.value}
						placeholder="输入数值..."
					/>
				{:else if getFieldType(condition.field) === 'date'}
					<input
						type="date"
						class="flex-1 rounded border border-ink-200 bg-white px-2 py-1.5 text-xs dark:border-ink-600 dark:bg-ink-800"
						bind:value={condition.value}
					/>
				{:else}
					<input
						type="text"
						class="flex-1 rounded border border-ink-200 bg-white px-2 py-1.5 text-xs dark:border-ink-600 dark:bg-ink-800"
						bind:value={condition.value}
						placeholder="输入文字..."
					/>
				{/if}

				<!-- Remove button -->
				<button
					class="flex-shrink-0 rounded p-1 text-ink-400 hover:bg-red-50 hover:text-red-500 dark:hover:bg-red-900/20"
					onclick={() => removeCondition(index)}
					disabled={conditions.length <= 1}
				>
					<X class="h-3.5 w-3.5" />
				</button>
			</div>
		{/each}
	</div>

	<!-- Actions -->
	<div class="flex items-center justify-between border-t border-ink-100 pt-3 dark:border-ink-700">
		<button
			class="flex items-center gap-1 rounded px-2 py-1 text-xs text-ink-500 hover:bg-ink-100 dark:hover:bg-ink-700"
			onclick={addCondition}
		>
			<Plus class="h-3 w-3" />
			添加条件
		</button>

		<div class="flex items-center gap-2">
			<button
				class="flex items-center gap-1 rounded px-2 py-1 text-xs text-ink-500 hover:bg-ink-100 dark:hover:bg-ink-700"
				onclick={() => showSaveDialog = !showSaveDialog}
			>
				<Save class="h-3 w-3" />
				保存
			</button>
			<button
				class="rounded bg-accent-500 px-3 py-1 text-xs font-medium text-white hover:bg-accent-600"
				onclick={apply}
			>
				应用筛选
			</button>
		</div>
	</div>

	<!-- Save dialog -->
	{#if showSaveDialog}
		<div class="flex items-center gap-2 border-t border-ink-100 pt-3 dark:border-ink-700">
			<input
				type="text"
				class="flex-1 rounded border border-ink-200 bg-white px-2 py-1.5 text-xs dark:border-ink-600 dark:bg-ink-800"
				bind:value={filterName}
				placeholder="筛选器名称..."
			/>
			<button
				class="rounded bg-accent-500 px-3 py-1 text-xs font-medium text-white hover:bg-accent-600 disabled:opacity-50"
				onclick={save}
				disabled={!filterName.trim()}
			>
				保存
			</button>
		</div>
	{/if}
</div>
