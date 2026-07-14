<script lang="ts">
	import { createQuery, createMutation, useQueryClient } from '@tanstack/svelte-query';
	import { api } from '$services/api';
	import { queryKeys } from '$lib/queries';
	import { toast } from 'svelte-sonner';
	import { Edit3, Check, X, RotateCcw, Save, BookOpen } from 'lucide-svelte';

	const queryClient = useQueryClient();

	const books = createQuery(() => ({
		queryKey: queryKeys.books.all,
		queryFn: () => api.getBooks({ per_page: 200 }),
	}));

	let selectedIds = $state<Set<string>>(new Set());
	let selectAll = $state(false);

	// Batch edit state
	let showBatchEdit = $state(false);
	let batchGenre = $state('');
	let batchTags = $state('');
	let batchStatus = $state('');
	let batchLanguage = $state('');

	function toggleSelect(id: string) {
		const next = new Set(selectedIds);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		selectedIds = next;
	}

	function toggleSelectAll() {
		if (selectAll) {
			selectedIds = new Set();
		} else {
			selectedIds = new Set((books.data?.data ?? []).map(b => b.id));
		}
		selectAll = !selectAll;
	}

	function handleBookRowKeydown(event: KeyboardEvent, id: string) {
		if (event.target instanceof HTMLInputElement) return;
		if (event.key !== 'Enter' && event.key !== ' ') return;
		event.preventDefault();
		toggleSelect(id);
	}

	const batchUpdate = createMutation(() => ({
		mutationFn: async () => {
			const metadata: Record<string, unknown> = {};
			if (batchGenre) metadata.genres = batchGenre.split(',').map(g => g.trim()).filter(Boolean);
			if (batchTags) metadata.tags = batchTags.split(',').map(t => t.trim()).filter(Boolean);
			if (batchStatus) metadata.status = batchStatus;
			if (batchLanguage) metadata.language = batchLanguage;

			const promises = Array.from(selectedIds).map(id =>
				api.updateBook(id, metadata)
			);
			await Promise.allSettled(promises);
		},
		onSuccess: () => {
			toast.success(`已更新 ${selectedIds.size} 本书的元数据`);
			queryClient.invalidateQueries({ queryKey: queryKeys.books.all });
			showBatchEdit = false;
			selectedIds = new Set();
		},
		onError: () => {
			toast.error('部分更新失败');
		},
	}));
</script>

<svelte:head>
	<title>Nova Reader — 批量编辑</title>
</svelte:head>

<div class="p-6 space-y-6 animate-fade-in">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">批量编辑元数据</h1>
			<p class="mt-1 text-sm text-ink-400">选择多本书进行批量修改标签、类型、状态</p>
		</div>

		{#if selectedIds.size > 0}
			<button
				class="flex items-center gap-2 px-4 py-2 rounded-lg bg-amber-500 text-ink-950 font-medium text-sm hover:bg-amber-400 transition-colors"
				onclick={() => showBatchEdit = true}
				type="button"
			>
				<Edit3 class="w-4 h-4" />
				编辑选中 ({selectedIds.size})
			</button>
		{/if}
	</div>

	<!-- Selection controls -->
	<div class="flex items-center gap-4 py-2 border-b border-ink-800/50">
		<label class="flex items-center gap-2 text-sm text-ink-300 cursor-pointer">
			<input
				type="checkbox"
				checked={selectAll}
				onchange={toggleSelectAll}
				class="rounded border-ink-700 bg-ink-800 text-amber-500 focus:ring-amber-500"
			/>
			全选
		</label>
		<span class="text-xs text-ink-500">
			{selectedIds.size} / {books.data?.data?.length ?? 0} 已选
		</span>
	</div>

	<!-- Book List (compact table format) -->
	{#if books.isLoading}
		<div class="space-y-2">
			{#each Array(10) as _}
				<div class="h-14 rounded-lg bg-ink-900 animate-pulse"></div>
			{/each}
		</div>
	{:else if books.data?.data}
		<div class="space-y-1">
			{#each books.data.data as book}
				{@const selected = selectedIds.has(book.id)}
				<div
					class="flex items-center gap-3 px-4 py-3 rounded-lg border transition-all cursor-pointer {selected ? 'border-amber-500/40 bg-amber-500/5' : 'border-ink-800/30 bg-ink-900/50 hover:bg-ink-900'}"
					onclick={() => toggleSelect(book.id)}
					onkeydown={(event) => handleBookRowKeydown(event, book.id)}
					role="button"
					tabindex="0"
					aria-pressed={selected}
				>
					<input
						type="checkbox"
						checked={selected}
						aria-label="选择《{book.title}》"
						class="rounded border-ink-700 bg-ink-800 text-amber-500 focus:ring-amber-500"
						onclick={(e) => e.stopPropagation()}
						onchange={() => toggleSelect(book.id)}
					/>

					<div class="flex-1 min-w-0 flex items-center gap-4">
						<div class="flex-1 min-w-0">
							<p class="text-sm text-ink-100 truncate">{book.title}</p>
							<p class="text-xs text-ink-500 truncate">
								{book.author ?? '未知作者'}
								{#if book.series_name} · {book.series_name}{/if}
							</p>
						</div>

						<!-- Current metadata pills -->
						<div class="hidden sm:flex items-center gap-1.5 shrink-0">
							{#if book.format}
								<span class="px-1.5 py-0.5 text-[10px] rounded bg-ink-800 text-ink-400">
									{book.format}
								</span>
							{/if}
							{#if book.status}
								<span class="px-1.5 py-0.5 text-[10px] rounded bg-ink-800 text-ink-400">
									{book.status}
								</span>
							{/if}
							{#if book.language}
								<span class="px-1.5 py-0.5 text-[10px] rounded bg-ink-800 text-ink-400">
									{book.language}
								</span>
							{/if}
						</div>

						<span class="text-xs text-ink-600 shrink-0 w-20 text-right">
							{(book.word_count ?? 0) >= 10000
								? `${((book.word_count ?? 0) / 10000).toFixed(1)}万字`
								: `${book.word_count ?? 0}字`}
						</span>
					</div>
				</div>
			{/each}
		</div>
	{:else}
		<div class="text-center py-16">
			<BookOpen class="w-12 h-12 text-ink-700 mx-auto mb-4" />
			<p class="text-ink-400">书库为空</p>
		</div>
	{/if}
</div>

<!-- Batch Edit Modal -->
{#if showBatchEdit}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
		<div role="dialog" aria-modal="true" aria-label="批量修改书籍元数据" tabindex="-1" class="w-full max-w-md rounded-2xl border border-ink-700 bg-ink-900 p-6 shadow-2xl">
			<div class="flex items-center justify-between mb-4">
				<h3 class="text-lg font-semibold text-ink-100">批量修改 ({selectedIds.size} 本)</h3>
				<button type="button" aria-label="关闭批量修改弹窗" class="text-ink-500 hover:text-ink-300" onclick={() => showBatchEdit = false}>
					<X class="w-5 h-5" />
				</button>
			</div>

			<p class="text-xs text-ink-500 mb-4">留空的字段将不会被修改。</p>

			<div class="space-y-4">
				<div>
					<label for="batch-status" class="block text-sm text-ink-300 mb-1">状态</label>
					<select id="batch-status" bind:value={batchStatus} class="w-full rounded-lg bg-ink-800 border border-ink-700 px-3 py-2 text-sm text-ink-100">
						<option value="">不修改</option>
						<option value="unread">未读</option>
						<option value="reading">在读</option>
						<option value="completed">已读完</option>
						<option value="on_hold">搁置</option>
						<option value="dropped">弃书</option>
					</select>
				</div>

				<div>
					<label for="batch-language" class="block text-sm text-ink-300 mb-1">语言</label>
					<select id="batch-language" bind:value={batchLanguage} class="w-full rounded-lg bg-ink-800 border border-ink-700 px-3 py-2 text-sm text-ink-100">
						<option value="">不修改</option>
						<option value="zh">中文</option>
						<option value="en">英文</option>
						<option value="ja">日文</option>
						<option value="ko">韩文</option>
					</select>
				</div>

				<div>
					<label for="batch-genre" class="block text-sm text-ink-300 mb-1">题材 (逗号分隔，追加)</label>
					<input
						id="batch-genre"
						type="text"
						bind:value={batchGenre}
						placeholder="玄幻, 都市"
						class="w-full rounded-lg bg-ink-800 border border-ink-700 px-3 py-2 text-sm text-ink-100"
					/>
				</div>

				<div>
					<label for="batch-tags" class="block text-sm text-ink-300 mb-1">标签 (逗号分隔，追加)</label>
					<input
						id="batch-tags"
						type="text"
						bind:value={batchTags}
						placeholder="推荐, 经典"
						class="w-full rounded-lg bg-ink-800 border border-ink-700 px-3 py-2 text-sm text-ink-100"
					/>
				</div>
			</div>

			<div class="mt-6 flex justify-end gap-3">
				<button
					type="button"
					class="px-4 py-2 text-sm text-ink-400 hover:text-ink-200 transition-colors"
					onclick={() => showBatchEdit = false}
				>
					取消
				</button>
				<button
					type="button"
					class="flex items-center gap-2 px-4 py-2 text-sm rounded-lg bg-amber-500 text-ink-950 font-medium hover:bg-amber-400 disabled:opacity-50 transition-colors"
					disabled={batchUpdate.isPending}
					onclick={() => batchUpdate.mutate()}
				>
					{#if batchUpdate.isPending}
						<span class="animate-spin w-4 h-4 border-2 border-ink-950/30 border-t-ink-950 rounded-full"></span>
					{:else}
						<Save class="w-4 h-4" />
					{/if}
					应用修改
				</button>
			</div>
		</div>
	</div>
{/if}
