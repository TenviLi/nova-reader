<script lang="ts">
	import { Search, LayoutGrid, List, Table, Plus, Calendar } from 'lucide-svelte';

	let { viewMode = $bindable('grid'), searchQuery = $bindable('') } = $props<{
		viewMode: 'grid' | 'list' | 'table' | 'timeline';
		searchQuery: string;
	}>();
</script>

	<div class="flex flex-col items-start gap-3 sm:flex-row sm:items-center sm:justify-between">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">所有书籍</h1>
			<p class="mt-0.5 text-sm text-ink-400">跨书库浏览、筛选和批量整理</p>
		</div>

		<div class="flex w-full flex-wrap items-center gap-3 sm:w-auto">
		<!-- Search -->
		<div class="relative">
			<Search size={16} strokeWidth={2} class="absolute left-3 top-1/2 -translate-y-1/2 text-ink-400" />
				<input
						type="text"
						bind:value={searchQuery}
						aria-label="筛选书籍"
						name="library-search"
						autocomplete="off"
						placeholder="筛选书籍…"
						class="w-full rounded-lg border border-ink-700/50 bg-ink-900/50 py-2 pl-9 pr-4 text-sm text-ink-100 placeholder-ink-500 outline-none transition-colors focus:border-accent-500/30 focus-visible:ring-2 focus-visible:ring-accent-400/70 sm:w-56"
					/>
		</div>

		<!-- View mode toggle -->
		<div class="flex rounded-lg border border-ink-700/50 bg-ink-900/50 p-0.5">
				<button
					type="button"
					onclick={() => viewMode = 'grid'}
					aria-label="网格视图"
					aria-pressed={viewMode === 'grid'}
					class="rounded-md px-2.5 py-1.5 text-xs transition-colors"
				class:bg-ink-700={viewMode === 'grid'}
				class:text-ink-100={viewMode === 'grid'}
				class:text-ink-400={viewMode !== 'grid'}
			>
				<LayoutGrid size={16} strokeWidth={2} />
			</button>
				<button
					type="button"
					onclick={() => viewMode = 'list'}
					aria-label="列表视图"
					aria-pressed={viewMode === 'list'}
					class="rounded-md px-2.5 py-1.5 text-xs transition-colors"
				class:bg-ink-700={viewMode === 'list'}
				class:text-ink-100={viewMode === 'list'}
				class:text-ink-400={viewMode !== 'list'}
			>
				<List size={16} strokeWidth={2} />
			</button>
				<button
					type="button"
					onclick={() => viewMode = 'table'}
					aria-label="表格视图"
					aria-pressed={viewMode === 'table'}
					class="rounded-md px-2.5 py-1.5 text-xs transition-colors"
				class:bg-ink-700={viewMode === 'table'}
				class:text-ink-100={viewMode === 'table'}
				class:text-ink-400={viewMode !== 'table'}
			>
				<Table size={16} strokeWidth={2} />
			</button>
				<button
					type="button"
					onclick={() => viewMode = 'timeline'}
					aria-label="时间轴视图"
					aria-pressed={viewMode === 'timeline'}
					class="rounded-md px-2.5 py-1.5 text-xs transition-colors"
				class:bg-ink-700={viewMode === 'timeline'}
				class:text-ink-100={viewMode === 'timeline'}
				class:text-ink-400={viewMode !== 'timeline'}
				title="时间轴"
			>
				<Calendar size={16} strokeWidth={2} />
			</button>
		</div>

		<!-- Library management -->
		<a
			href="/libraries"
			aria-label="打开书库管理"
			class="inline-flex items-center gap-2 rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-ink-950 hover:bg-accent-400 transition-colors"
		>
			<Plus size={16} strokeWidth={2} />
			书库管理
		</a>
	</div>
</div>
