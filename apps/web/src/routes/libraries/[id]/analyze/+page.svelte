<script lang="ts">
import { getErrorMessage } from '$lib/utils';
	import { page } from '$app/stores';
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import { toast } from 'svelte-sonner';
	import { ArrowLeft, Brain, BookOpen, Sparkles, Clock, CheckCircle, AlertCircle, Loader2 } from 'lucide-svelte';
	import { Button } from '$lib/components/ui/button';

	import type { Library, Book } from '$types/models';

	const libraryId = $page.params.id!;

	let library = $state<Library | null>(null);
	let loading = $state(true);
	let analyzing = $state(false);
	let result = $state<{ message: string; tasks_queued?: number; total_unanalyzed?: number } | null>(null);
	let books = $state<Book[]>([]);
	let analyzedCount = $state(0);
	let totalCount = $state(0);

	onMount(async () => {
		try {
			const [libraryData, booksData] = await Promise.all([
				api.getLibrary(libraryId),
				api.getBooks({ library_id: libraryId, per_page: 200 }),
			]);
			library = libraryData ?? null;
			books = booksData?.data ?? [];
			totalCount = books.length;

			// Count books that have entities (already analyzed)
			let analyzed = 0;
			for (const book of books) {
				if (book.has_entities || (book.entity_count ?? 0) > 0) analyzed++;
			}
			analyzedCount = analyzed;
		} catch (e: unknown) {
			toast.error('加载书库信息失败');
		} finally {
			loading = false;
		}
	});

	async function startAnalysis() {
		analyzing = true;
		try {
			const res = await api.analyzeLibrary(libraryId);
			result = res;
			toast.success(`已提交 ${res.tasks_queued ?? 0} 个分析任务`);
		} catch (e: unknown) {
			toast.error(`分析失败: ${getErrorMessage(e)}`);
		} finally {
			analyzing = false;
		}
	}
</script>

<svelte:head>
	<title>AI 分析 — {library?.name ?? 'Nova Reader'}</title>
</svelte:head>

<div class="mx-auto max-w-4xl px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	{#if loading}
		<div class="space-y-4">
			<div class="h-8 w-48 rounded-lg bg-ink-900/50 animate-pulse"></div>
			<div class="h-64 rounded-xl bg-ink-900/50 animate-pulse"></div>
		</div>
	{:else if library}
		<!-- Header -->
		<div class="flex items-center gap-3">
			<a
				href="/libraries/{libraryId}"
				class="inline-flex items-center gap-1.5 text-sm text-ink-400 hover:text-ink-200 transition-colors"
			>
				<ArrowLeft size={16} />
				返回书库
			</a>
		</div>

		<div class="flex items-center gap-4">
			<div class="flex h-12 w-12 items-center justify-center rounded-xl bg-purple-500/10 ring-1 ring-purple-500/20">
				<Brain size={24} class="text-purple-400" />
			</div>
			<div>
				<h1 class="text-2xl font-bold text-ink-50">AI 智能分析</h1>
				<p class="text-sm text-ink-400">对「{library.name}」中的书籍进行实体提取、摘要生成和标签分析</p>
			</div>
		</div>

		<!-- Stats Overview -->
		<div class="grid grid-cols-3 gap-4">
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/50 p-5 text-center">
				<BookOpen size={20} class="mx-auto text-ink-400 mb-2" />
				<p class="text-2xl font-bold text-ink-100">{totalCount}</p>
				<p class="text-xs text-ink-400 mt-1">总书籍</p>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/50 p-5 text-center">
				<CheckCircle size={20} class="mx-auto text-green-400 mb-2" />
				<p class="text-2xl font-bold text-green-400">{analyzedCount}</p>
				<p class="text-xs text-ink-400 mt-1">已分析</p>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/50 p-5 text-center">
				<Clock size={20} class="mx-auto text-amber-400 mb-2" />
				<p class="text-2xl font-bold text-amber-400">{totalCount - analyzedCount}</p>
				<p class="text-xs text-ink-400 mt-1">待分析</p>
			</div>
		</div>

		<!-- Progress Bar -->
		{#if totalCount > 0}
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/50 p-5">
				<div class="flex items-center justify-between mb-2">
					<span class="text-sm text-ink-300">分析进度</span>
					<span class="text-sm font-medium text-ink-200">{Math.round(analyzedCount / totalCount * 100)}%</span>
				</div>
				<div class="h-2.5 w-full overflow-hidden rounded-full bg-ink-800">
					<div
						class="h-full rounded-full bg-gradient-to-r from-purple-500 to-accent-500 transition-all duration-500"
						style="width: {(analyzedCount / totalCount) * 100}%"
					></div>
				</div>
			</div>
		{/if}

		<!-- Analysis Features -->
		<div class="rounded-xl border border-ink-800/50 bg-ink-900/50 p-6 space-y-4">
			<h2 class="text-base font-semibold text-ink-100">分析功能</h2>
			<div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
				<div class="flex items-start gap-3 p-3 rounded-lg bg-ink-800/30">
					<Sparkles size={18} class="text-purple-400 shrink-0 mt-0.5" />
					<div>
						<p class="text-sm font-medium text-ink-200">实体提取</p>
						<p class="text-xs text-ink-500">识别人物、地点、组织等命名实体</p>
					</div>
				</div>
				<div class="flex items-start gap-3 p-3 rounded-lg bg-ink-800/30">
					<Sparkles size={18} class="text-blue-400 shrink-0 mt-0.5" />
					<div>
						<p class="text-sm font-medium text-ink-200">智能摘要</p>
						<p class="text-xs text-ink-500">生成章节和全书摘要</p>
					</div>
				</div>
				<div class="flex items-start gap-3 p-3 rounded-lg bg-ink-800/30">
					<Sparkles size={18} class="text-amber-400 shrink-0 mt-0.5" />
					<div>
						<p class="text-sm font-medium text-ink-200">标签生成</p>
						<p class="text-xs text-ink-500">根据内容自动生成分类标签</p>
					</div>
				</div>
				<div class="flex items-start gap-3 p-3 rounded-lg bg-ink-800/30">
					<Sparkles size={18} class="text-emerald-400 shrink-0 mt-0.5" />
					<div>
						<p class="text-sm font-medium text-ink-200">关系图谱</p>
						<p class="text-xs text-ink-500">构建人物关系网络</p>
					</div>
				</div>
			</div>
		</div>

		<!-- Action -->
		<div class="flex items-center justify-between rounded-xl border border-purple-500/20 bg-purple-500/5 p-6">
			<div>
				<p class="text-sm font-medium text-ink-200">
					{#if totalCount - analyzedCount > 0}
						{totalCount - analyzedCount} 本书籍待分析
					{:else}
						所有书籍已完成分析
					{/if}
				</p>
				<p class="text-xs text-ink-500 mt-1">分析将使用 AI 服务，可能需要一些时间</p>
			</div>
			<Button
				onclick={startAnalysis}
				disabled={analyzing || totalCount - analyzedCount === 0}
				class="bg-purple-500 hover:bg-purple-400 text-white disabled:opacity-50"
			>
				{#if analyzing}
					<Loader2 size={16} class="animate-spin mr-2" />
					分析中...
				{:else}
					<Brain size={16} class="mr-2" />
					开始分析
				{/if}
			</Button>
		</div>

		<!-- Result -->
		{#if result}
			<div class="rounded-xl border border-green-500/20 bg-green-500/5 p-5">
				<div class="flex items-center gap-2 mb-2">
					<CheckCircle size={16} class="text-green-400" />
					<span class="text-sm font-medium text-green-300">分析任务已提交</span>
				</div>
				<div class="text-sm text-ink-400 space-y-1">
					<p>待分析书籍: {result.total_unanalyzed ?? 0}</p>
					<p>已排队任务: {result.tasks_queued ?? 0}</p>
				</div>
			</div>
		{/if}
	{:else}
		<div class="text-center py-20">
			<AlertCircle size={48} class="mx-auto text-ink-600 mb-3" />
			<p class="text-ink-300">书库不存在</p>
			<a href="/libraries" class="mt-3 inline-block text-sm text-accent-400 hover:text-accent-300">返回书库管理</a>
		</div>
	{/if}
</div>
