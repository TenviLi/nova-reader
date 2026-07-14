<script lang="ts">
	let { sortBy = $bindable('updated_at'), filterStatus = $bindable(null), filterLanguage = $bindable(null), filterFormat = $bindable(null) } = $props<{
		sortBy: string;
		filterStatus: string | null;
		filterLanguage: string | null;
		filterFormat?: string | null;
	}>();

	const sortOptions = [
		{ value: 'updated_at', label: '最近更新' },
		{ value: 'created_at', label: '最近添加' },
		{ value: 'title', label: '标题' },
		{ value: 'author', label: '作者' },
		{ value: 'progress', label: '阅读进度' },
		{ value: 'word_count', label: '字数' },
	];

	const statusOptions = [
		{ value: null, label: '全部状态' },
		{ value: 'unread', label: '未读' },
		{ value: 'reading', label: '在读' },
		{ value: 'completed', label: '已读' },
		{ value: 'on_hold', label: '搁置' },
		{ value: 'dropped', label: '弃读' },
	];

	const languageOptions = [
		{ value: null, label: '全部语言' },
		{ value: 'zh', label: '中文' },
		{ value: 'en', label: 'English' },
		{ value: 'ja', label: '日本語' },
		{ value: 'ko', label: '한국어' },
	];

	const formatOptions = [
		{ value: null, label: '全部格式' },
		{ value: 'epub', label: 'EPUB' },
		{ value: 'txt', label: 'TXT' },
		{ value: 'pdf', label: 'PDF' },
		{ value: 'docx', label: 'DOCX' },
		{ value: 'doc', label: 'DOC' },
		{ value: 'md', label: 'Markdown' },
		{ value: 'html', label: 'HTML' },
	];
</script>

<div class="flex flex-wrap items-center gap-3">
	<!-- Sort -->
	<select
		bind:value={sortBy}
		name="book-sort"
		aria-label="书籍排序"
		class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-1.5 text-sm text-ink-200 transition-colors focus:border-accent-500/30 focus:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/60"
	>
		{#each sortOptions as opt}
			<option value={opt.value}>{opt.label}</option>
		{/each}
	</select>

	<!-- Status filter -->
	<select
		bind:value={filterStatus}
		name="book-reading-status-filter"
		aria-label="阅读状态筛选"
		class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-1.5 text-sm text-ink-200 transition-colors focus:border-accent-500/30 focus:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/60"
	>
		{#each statusOptions as opt}
			<option value={opt.value}>{opt.label}</option>
		{/each}
	</select>

	<!-- Language filter -->
	<select
		bind:value={filterLanguage}
		name="book-language-filter"
		aria-label="语言筛选"
		class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-1.5 text-sm text-ink-200 transition-colors focus:border-accent-500/30 focus:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/60"
	>
		{#each languageOptions as opt}
			<option value={opt.value}>{opt.label}</option>
		{/each}
	</select>

	<!-- Format filter -->
	<select
		bind:value={filterFormat}
		name="book-format-filter"
		aria-label="格式筛选"
		class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-1.5 text-sm text-ink-200 transition-colors focus:border-accent-500/30 focus:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/60"
	>
		{#each formatOptions as opt}
			<option value={opt.value}>{opt.label}</option>
		{/each}
	</select>

	<!-- Active filter badges -->
	{#if filterStatus || filterLanguage || filterFormat}
		<button
			type="button"
			onclick={() => { filterStatus = null; filterLanguage = null; filterFormat = null; }}
			class="rounded px-2 py-1 text-xs text-ink-400 transition-colors hover:text-accent-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
		>
			清除筛选
		</button>
	{/if}
</div>
