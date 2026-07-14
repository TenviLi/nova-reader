<script lang="ts">
	import { api } from '$services/api';
	import { getErrorMessage } from '$lib/utils';
	import SemanticMarkersPanel from '$components/analytics/SemanticMarkersPanel.svelte';
	import { toast } from 'svelte-sonner';
	import { BarChart3, Brain, Languages, Network, Radar, RefreshCw, Search, Sparkles, Tags, Users } from 'lucide-svelte';

	let {
		bookId,
		bookTitle,
		chapterIndex,
		totalChapters,
		content,
		entityCount,
		annotationCount,
		immersiveMode = $bindable(false),
	} = $props<{
		bookId: string;
		bookTitle: string;
		chapterIndex: number;
		totalChapters: number;
		content: string;
		entityCount: number;
		annotationCount: number;
		immersiveMode?: boolean;
	}>();

	let runningAction = $state<string | null>(null);

	let estimatedMinutes = $derived(Math.max(1, Math.ceil((content?.length ?? 0) / 800)));
	let chapterLabel = $derived(totalChapters > 0 ? `${chapterIndex + 1} / ${totalChapters}` : '未解析');

	async function runAction(action: 'deep_analysis' | 'semantic_tags' | 'embeddings' | 'batch') {
		runningAction = action;
		try {
			if (action === 'deep_analysis') {
				await api.submitPipeline(bookId, 'deep_analysis');
				toast.success('深度分析任务已提交');
			} else if (action === 'semantic_tags') {
				await api.computeBookTags(bookId);
				toast.success('智能标签计算任务已提交');
			} else if (action === 'embeddings') {
				await api.aiIngestEmbeddings(bookId);
				toast.success('语义索引任务已提交');
			} else {
				await api.aiBatchProcess(bookId);
				toast.success('全书 AI 处理已完成');
			}
		} catch (err) {
			toast.error(getErrorMessage(err) ?? '任务提交失败');
		} finally {
			runningAction = null;
		}
	}

	let destinations = $derived.by(() => [
		{ label: '智能标签画像', href: `/library/${bookId}?tab=semantic`, icon: Radar, description: '查看书籍画像、热力图和标签浓度' },
		{ label: '人物志', href: `/library/${bookId}?tab=characters`, icon: Users, description: '查看本书人物与实体列表' },
		{ label: '知识图谱', href: `/graph?book_id=${bookId}`, icon: Network, description: '进入书籍相关实体关系图' },
		{ label: '深度分析', href: `/library/${bookId}?tab=analysis`, icon: BarChart3, description: '书籍级结构、风格和风险分析' },
		{ label: '相似推荐', href: `/library/${bookId}?tab=similar`, icon: Sparkles, description: '基于标签和语义寻找相近书籍' },
		{ label: '全库检索', href: `/search?q=${encodeURIComponent(bookTitle)}&mode=hybrid`, icon: Search, description: '用书名作为线索发起智能搜索' },
	]);
</script>

<div class="space-y-4">
	<section class="rounded-xl border border-ink-800/60 bg-ink-900/50 p-3">
		<div class="flex items-center justify-between gap-3">
			<div>
				<p class="text-xs text-ink-500">当前章节</p>
				<p class="mt-0.5 text-sm font-medium text-ink-100">第 {chapterIndex + 1} 章</p>
			</div>
			<span class="rounded-full bg-accent-500/10 px-2 py-0.5 text-[10px] font-medium text-accent-300">{chapterLabel}</span>
		</div>
		<div class="mt-3 grid grid-cols-3 gap-2 text-center">
			<div class="rounded-lg bg-ink-950/50 px-2 py-2">
				<p class="text-[10px] text-ink-500">预计</p>
				<p class="mt-0.5 text-sm font-semibold text-ink-200">{estimatedMinutes}m</p>
			</div>
			<div class="rounded-lg bg-ink-950/50 px-2 py-2">
				<p class="text-[10px] text-ink-500">实体</p>
				<p class="mt-0.5 text-sm font-semibold text-ink-200">{entityCount}</p>
			</div>
			<div class="rounded-lg bg-ink-950/50 px-2 py-2">
				<p class="text-[10px] text-ink-500">批注</p>
				<p class="mt-0.5 text-sm font-semibold text-ink-200">{annotationCount}</p>
			</div>
		</div>
	</section>

	<section class="space-y-2">
		<div class="flex items-center gap-2 px-1 text-xs font-medium text-ink-500">
			<Brain size={13} /> 阅读器能力
		</div>
		<button
			type="button"
			onclick={() => immersiveMode = !immersiveMode}
			aria-pressed={immersiveMode}
			aria-label={immersiveMode ? '关闭沉浸式翻译' : '开启沉浸式翻译'}
			class="flex w-full items-center gap-3 rounded-xl border p-3 text-left transition-colors {immersiveMode ? 'border-accent-500/30 bg-accent-500/10' : 'border-ink-800/60 bg-ink-900/40 hover:border-ink-700/70 hover:bg-ink-900'}"
		>
			<div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-ink-950/60 text-accent-300">
				<Languages size={17} />
			</div>
			<div class="min-w-0 flex-1">
				<p class="text-sm font-medium text-ink-100">沉浸式翻译</p>
				<p class="mt-0.5 text-xs text-ink-500">原文、双语、译文、悬浮释义</p>
			</div>
			<span class="text-[10px] {immersiveMode ? 'text-accent-300' : 'text-ink-600'}">{immersiveMode ? '已开' : '关闭'}</span>
		</button>
	</section>

	<SemanticMarkersPanel {bookId} currentChapterIndex={chapterIndex} compact />

	<section class="space-y-2">
		<div class="flex items-center gap-2 px-1 text-xs font-medium text-ink-500">
			<RefreshCw size={13} /> 一键任务
		</div>
		<div class="grid grid-cols-2 gap-2">
			<button type="button" onclick={() => runAction('deep_analysis')} disabled={!!runningAction} aria-label="提交深度分析任务" class="rounded-lg border border-ink-800/60 bg-ink-900/40 p-2 text-left transition-colors hover:border-accent-500/30 disabled:opacity-50">
				<BarChart3 size={15} class="mb-1 text-accent-300" />
				<span class="block text-xs text-ink-200">深度分析</span>
			</button>
			<button type="button" onclick={() => runAction('semantic_tags')} disabled={!!runningAction} aria-label="提交智能标签任务" class="rounded-lg border border-ink-800/60 bg-ink-900/40 p-2 text-left transition-colors hover:border-accent-500/30 disabled:opacity-50">
				<Tags size={15} class="mb-1 text-emerald-300" />
				<span class="block text-xs text-ink-200">智能标签</span>
			</button>
			<button type="button" onclick={() => runAction('embeddings')} disabled={!!runningAction} aria-label="提交语义索引任务" class="rounded-lg border border-ink-800/60 bg-ink-900/40 p-2 text-left transition-colors hover:border-accent-500/30 disabled:opacity-50">
				<Network size={15} class="mb-1 text-cyan-300" />
				<span class="block text-xs text-ink-200">语义索引</span>
			</button>
			<button type="button" onclick={() => runAction('batch')} disabled={!!runningAction} aria-label="提交全书 AI 处理任务" class="rounded-lg border border-ink-800/60 bg-ink-900/40 p-2 text-left transition-colors hover:border-accent-500/30 disabled:opacity-50">
				<Sparkles size={15} class="mb-1 text-amber-300" />
				<span class="block text-xs text-ink-200">全书处理</span>
			</button>
		</div>
		{#if runningAction}
			<p class="px-1 text-[11px] text-ink-500">任务处理中，请稍候...</p>
		{/if}
	</section>

	<section class="space-y-2">
		<div class="flex items-center gap-2 px-1 text-xs font-medium text-ink-500">
			<Radar size={13} /> 书籍工作台
		</div>
		<div class="space-y-1">
			{#each destinations as item}
				{@const Icon = item.icon}
				<a href={item.href} class="group flex items-center gap-3 rounded-lg px-3 py-2 transition-colors hover:bg-ink-800/60">
					<Icon size={15} class="shrink-0 text-ink-500 group-hover:text-accent-300" />
					<span class="min-w-0 flex-1">
						<span class="block text-xs font-medium text-ink-200">{item.label}</span>
						<span class="block truncate text-[11px] text-ink-600 group-hover:text-ink-500">{item.description}</span>
					</span>
				</a>
			{/each}
		</div>
	</section>
</div>
