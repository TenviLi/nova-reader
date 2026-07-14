<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import { api } from '$services/api';
	import { browser } from '$app/environment';
	import { BookOpen, Search, ChevronRight, Library } from 'lucide-svelte';

	let searchQuery = $state('');
	let statusFilter = $state('all');
	let sortBy = $state('name');

	const series = createQuery(() => ({
		queryKey: ['series', 'list', searchQuery, statusFilter, sortBy],
		queryFn: async () => {
			return await api.getSeriesList({
				search: searchQuery || undefined,
				status: statusFilter === 'all' ? undefined : statusFilter,
				sort_by: sortBy,
				sort_dir: sortBy === 'updated_at' || sortBy === 'created_at' || sortBy === 'book_count' ? 'desc' : 'asc',
			}) ?? [];
		},
		enabled: browser,
	}));

	const statusLabels: Record<string, string> = {
		ongoing: '连载中',
		completed: '已完结',
		hiatus: '暂停',
		cancelled: '弃坑',
		unknown: '未知',
	};

	const statusColors: Record<string, string> = {
		ongoing: 'text-emerald-400 bg-emerald-500/10',
		completed: 'text-blue-400 bg-blue-500/10',
		hiatus: 'text-amber-400 bg-amber-500/10',
		cancelled: 'text-red-400 bg-red-500/10',
		unknown: 'text-ink-400 bg-ink-700/50',
	};

	const sortOptions = [
		{ value: 'name', label: '按名称' },
		{ value: 'book_count', label: '按书籍数' },
		{ value: 'updated_at', label: '最近更新' },
		{ value: 'created_at', label: '最近创建' },
		{ value: 'status', label: '按状态' },
	];

	function formatWords(count: number): string {
		if (count >= 10000) return `${(count / 10000).toFixed(1)}万字`;
		if (count >= 1000) return `${(count / 1000).toFixed(0)}千字`;
		return `${count}字`;
	}
</script>

<svelte:head>
	<title>Nova Reader — 系列管理</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	<!-- Header -->
	<div>
		<h1 class="text-2xl font-bold text-ink-50">系列管理</h1>
		<p class="mt-1 text-sm text-ink-400">自动从目录结构识别的小说系列</p>
	</div>

	<!-- Filters -->
	<div class="flex flex-wrap items-center gap-4">
		<div class="relative flex-1 max-w-sm">
			<Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-ink-500" />
			<input
				type="text"
				bind:value={searchQuery}
				placeholder="搜索系列名..."
				class="w-full pl-9 pr-3 py-2 rounded-lg bg-ink-900 border border-ink-800 text-sm text-ink-100 placeholder:text-ink-600 focus:border-amber-500/50 focus:outline-none"
			/>
		</div>

		<div class="flex items-center gap-1 rounded-lg bg-ink-900 border border-ink-800 p-1">
			<button
				class="px-3 py-1.5 text-xs font-medium rounded-md transition-colors {statusFilter === 'all' ? 'bg-amber-500/10 text-amber-300' : 'text-ink-400 hover:text-ink-200'}"
				onclick={() => statusFilter = 'all'}
			>全部</button>
			{#each Object.entries(statusLabels) as [value, label]}
				<button
					class="px-3 py-1.5 text-xs font-medium rounded-md transition-colors {statusFilter === value ? 'bg-amber-500/10 text-amber-300' : 'text-ink-400 hover:text-ink-200'}"
					onclick={() => statusFilter = value}
				>{label}</button>
			{/each}
		</div>

		<select
			bind:value={sortBy}
			class="rounded-lg border border-ink-800 bg-ink-900 px-3 py-2 text-sm text-ink-200 outline-none focus:border-amber-500/50"
			aria-label="系列排序"
		>
			{#each sortOptions as option}
				<option value={option.value}>{option.label}</option>
			{/each}
		</select>
	</div>

	<!-- Series Grid -->
	{#if series.isLoading}
		<div class="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
			{#each Array(6) as _}
				<div class="h-36 rounded-xl bg-ink-900 border border-ink-800 animate-pulse"></div>
			{/each}
		</div>
	{:else if series.isError}
		<div class="text-center py-16">
			<Library class="w-12 h-12 text-red-700 mx-auto mb-4" />
			<p class="text-red-400">加载系列数据失败</p>
			<p class="text-sm text-ink-600 mt-1">{series.error?.message ?? '请稍后重试'}</p>
			<button
				onclick={() => series.refetch()}
				class="mt-4 rounded-lg border border-ink-700/50 px-4 py-2 text-sm text-ink-300 hover:bg-ink-800/50 transition-colors"
			>重新加载</button>
		</div>
	{:else if series.data && series.data.length > 0}
		<div class="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
			{#each series.data as s}
				<a
					href="/series/{s.id}"
					class="group rounded-xl border border-ink-800/50 bg-ink-900/80 overflow-hidden hover:border-amber-500/30 hover:bg-ink-900 transition-all"
				>
					<!-- Cover Mosaic -->
					<div class="h-24 grid grid-cols-2 gap-px bg-ink-800/50">
						{#each Array(4) as _, i}
							{#if s.book_covers?.[i]}
								<img src="/api/covers/{s.book_covers[i]}" alt="" class="h-full w-full object-cover" />
							{:else}
								<div class="flex items-center justify-center bg-ink-900">
									<BookOpen class="w-4 h-4 text-ink-700" />
								</div>
							{/if}
						{/each}
					</div>

					<div class="p-4">
						<div class="flex items-start justify-between">
							<div class="flex-1 min-w-0">
								<h3 class="text-sm font-semibold text-ink-100 group-hover:text-amber-300 transition-colors truncate">
									{s.name}
								</h3>
								{#if s.original_name && s.original_name !== s.name}
									<p class="text-xs text-ink-500 truncate mt-0.5">{s.original_name}</p>
								{/if}
							</div>
							<span class="shrink-0 inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium {statusColors[s.status] ?? statusColors.unknown}">
								{statusLabels[s.status] ?? '未知'}
							</span>
						</div>

						<div class="mt-3 flex items-center gap-4 text-xs text-ink-500">
							<span class="flex items-center gap-1">
								<BookOpen class="w-3 h-3" />
								{s.book_count} 本
							</span>
							<span>{formatWords(s.total_word_count)}</span>
						</div>
					</div>
				</a>
			{/each}
		</div>
	{:else}
		<div class="text-center py-16">
			<Library class="w-12 h-12 text-ink-700 mx-auto mb-4" />
			<p class="text-ink-400">暂无系列数据</p>
			<p class="text-sm text-ink-600 mt-1">添加书库目录后，系统会自动识别系列文件夹</p>
		</div>
	{/if}
</div>
