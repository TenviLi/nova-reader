<script lang="ts">
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import { toast } from 'svelte-sonner';
	import { Cpu, Globe, Zap, Database, ToggleLeft } from 'lucide-svelte';
	import { Input } from '$lib/components/ui/input';
	import { Button } from '$lib/components/ui/button';

	let loading = $state(true);
	let saving = $state(false);

	// AI Configuration
	let config = $state<{
		llm_endpoint: string;
		llm_model: string;
		llm_api_key: string;
		embedding_endpoint: string;
		embedding_model: string;
		reranker_endpoint: string;
		reranker_model: string;
		reranker_enabled: boolean;
		qdrant_url: string;
		features: Record<string, boolean>;
	}>({
		// LLM
		llm_endpoint: 'https://api.deepseek.com',
		llm_model: 'deepseek-chat',
		llm_api_key: '',
		// Embedding
		embedding_endpoint: 'http://127.0.0.1:1234',
		embedding_model: 'qwen3-embedding-0.6b',
		// Reranker
		reranker_endpoint: 'http://127.0.0.1:1234',
		reranker_model: 'qwen3-reranker-0.6b-mlx',
		reranker_enabled: true,
		// Vector DB
		qdrant_url: 'http://localhost:6333',
		// Feature toggles
		features: {
			ai_chat: true,
			ai_entities: true,
			ai_summarize: true,
			ai_translate: true,
			ai_style_analysis: true,
			ai_batch_process: true,
			semantic_search: true,
			knowledge_graph: true,
			reranker: true,
		}
	});

	onMount(async () => {
		try {
			const settings = await api.getSettings();
			if (settings?.ai) {
				config = { ...config, ...settings.ai };
			}
		} catch {
			// Use defaults
		} finally {
			loading = false;
		}
	});

	async function saveConfig() {
		saving = true;
		try {
			await api.updateSettings({ ai: config });
			toast.success('AI 配置已保存');
		} catch (e) {
			toast.error('保存失败', { description: e instanceof Error ? e.message : '未知错误' });
		} finally {
			saving = false;
		}
	}

	async function testEmbedding() {
		try {
			const resp = await fetch(`${config.embedding_endpoint}/v1/embeddings`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ input: ['测试文本'], model: config.embedding_model }),
			});
			if (resp.ok) {
				const data = await resp.json();
				const dim = data.data?.[0]?.embedding?.length ?? 0;
				toast.success(`嵌入服务正常`, { description: `维度: ${dim}` });
			} else {
				toast.error('嵌入服务异常', { description: `HTTP ${resp.status}` });
			}
		} catch (e) {
			toast.error('无法连接嵌入服务', { description: config.embedding_endpoint });
		}
	}

	async function testReranker() {
		try {
			const resp = await fetch(`${config.reranker_endpoint}/v1/rerank`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					model: config.reranker_model,
					query: '测试查询',
					documents: ['文档一', '文档二'],
					top_n: 2,
				}),
			});
			if (resp.ok) {
				toast.success('重排服务正常');
			} else {
				toast.error('重排服务异常', { description: `HTTP ${resp.status}` });
			}
		} catch (e) {
			toast.error('无法连接重排服务', { description: config.reranker_endpoint });
		}
	}
</script>

<svelte:head>
	<title>Nova Reader — AI 配置</title>
</svelte:head>

<div class="space-y-6 max-w-3xl">
	<div>
		<h1 class="text-xl font-bold text-ink-50">AI 服务配置</h1>
		<p class="text-sm text-ink-400 mt-1">管理 LLM、嵌入、重排模型和功能开关</p>
	</div>

	{#if loading}
		<div class="space-y-4">
			{#each Array(3) as _}
				<div class="h-32 rounded-xl bg-ink-900/50 animate-pulse"></div>
			{/each}
		</div>
	{:else}
		<!-- LLM Config -->
		<section class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5 space-y-4">
			<div class="flex items-center gap-2 mb-2">
				<Globe size={18} class="text-accent-400" />
				<h2 class="text-base font-semibold text-ink-100">LLM 大模型</h2>
			</div>
			<div class="grid grid-cols-1 md:grid-cols-2 gap-4">
				<div>
					<label for="llm-endpoint" class="text-xs text-ink-400 mb-1 block">API 端点</label>
					<Input id="llm-endpoint" bind:value={config.llm_endpoint} class="bg-ink-800/50 border-ink-700/60 text-ink-100" />
				</div>
				<div>
					<label for="llm-model" class="text-xs text-ink-400 mb-1 block">模型名称</label>
					<Input id="llm-model" bind:value={config.llm_model} class="bg-ink-800/50 border-ink-700/60 text-ink-100" />
				</div>
				<div class="md:col-span-2">
					<label for="llm-api-key" class="text-xs text-ink-400 mb-1 block">API Key</label>
					<Input id="llm-api-key" type="password" bind:value={config.llm_api_key} placeholder="sk-..." class="bg-ink-800/50 border-ink-700/60 text-ink-100" />
				</div>
			</div>
		</section>

		<!-- Embedding Config -->
		<section class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5 space-y-4">
			<div class="flex items-center justify-between mb-2">
				<div class="flex items-center gap-2">
					<Database size={18} class="text-emerald-400" />
					<h2 class="text-base font-semibold text-ink-100">嵌入模型 (Embedding)</h2>
				</div>
				<Button variant="outline" size="sm" onclick={testEmbedding} class="text-xs">
					测试连接
				</Button>
			</div>
			<div class="grid grid-cols-1 md:grid-cols-2 gap-4">
				<div>
					<label for="embedding-endpoint" class="text-xs text-ink-400 mb-1 block">服务端点</label>
					<Input id="embedding-endpoint" bind:value={config.embedding_endpoint} class="bg-ink-800/50 border-ink-700/60 text-ink-100" />
				</div>
				<div>
					<label for="embedding-model" class="text-xs text-ink-400 mb-1 block">模型名称</label>
					<Input id="embedding-model" bind:value={config.embedding_model} class="bg-ink-800/50 border-ink-700/60 text-ink-100" />
				</div>
			</div>
			<p class="text-xs text-ink-500">使用 OpenAI-compatible /v1/embeddings API 格式。支持 LM Studio / Ollama / vLLM。</p>
		</section>

		<!-- Reranker Config -->
		<section class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5 space-y-4">
			<div class="flex items-center justify-between mb-2">
				<div class="flex items-center gap-2">
					<Zap size={18} class="text-amber-400" />
					<h2 class="text-base font-semibold text-ink-100">重排模型 (Reranker)</h2>
				</div>
				<div class="flex items-center gap-2">
					<button
						onclick={() => config.reranker_enabled = !config.reranker_enabled}
						class="flex items-center gap-1.5 text-xs px-2 py-1 rounded {config.reranker_enabled ? 'bg-emerald-500/15 text-emerald-400' : 'bg-ink-800 text-ink-500'}"
					>
						<ToggleLeft size={14} />
						{config.reranker_enabled ? '已启用' : '已禁用'}
					</button>
					<Button variant="outline" size="sm" onclick={testReranker} class="text-xs" disabled={!config.reranker_enabled}>
						测试连接
					</Button>
				</div>
			</div>
			{#if config.reranker_enabled}
				<div class="grid grid-cols-1 md:grid-cols-2 gap-4">
					<div>
						<label for="reranker-endpoint" class="text-xs text-ink-400 mb-1 block">服务端点</label>
						<Input id="reranker-endpoint" bind:value={config.reranker_endpoint} class="bg-ink-800/50 border-ink-700/60 text-ink-100" />
					</div>
					<div>
						<label for="reranker-model" class="text-xs text-ink-400 mb-1 block">模型名称</label>
						<Input id="reranker-model" bind:value={config.reranker_model} class="bg-ink-800/50 border-ink-700/60 text-ink-100" />
					</div>
				</div>
				<p class="text-xs text-ink-500">Embedding 召回后使用 Reranker 精排，显著提升 RAG 准确率。使用 /v1/rerank API。</p>
			{/if}
		</section>

		<!-- Feature Toggles -->
		<section class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5 space-y-4">
			<div class="flex items-center gap-2 mb-2">
				<ToggleLeft size={18} class="text-violet-400" />
				<h2 class="text-base font-semibold text-ink-100">功能开关</h2>
			</div>
			<div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
				{#each Object.entries(config.features) as [key, enabled]}
					{@const labels: Record<string, string> = {
						ai_chat: 'AI 对话 (RAG)',
						ai_entities: '实体提取 (NER)',
						ai_summarize: '智能摘要',
						ai_translate: 'AI 翻译',
						ai_style_analysis: '风格分析',
						ai_batch_process: '批量 AI 处理',
						semantic_search: '语义搜索',
						knowledge_graph: '知识图谱',
						reranker: '重排优化',
					}}
					<label class="flex items-center justify-between rounded-lg border border-ink-800/40 bg-ink-900/50 px-4 py-3 cursor-pointer hover:border-ink-700/60 transition-colors">
						<span class="text-sm text-ink-200">{labels[key] ?? key}</span>
						<input
							type="checkbox"
							checked={enabled}
							onchange={() => config.features[key as keyof typeof config.features] = !enabled}
							class="h-4 w-4 rounded border-ink-600 bg-ink-800 text-accent-500 focus:ring-accent-500/30"
						/>
					</label>
				{/each}
			</div>
		</section>

		<!-- Vector DB -->
		<section class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5 space-y-4">
			<div class="flex items-center gap-2 mb-2">
				<Database size={18} class="text-cyan-400" />
				<h2 class="text-base font-semibold text-ink-100">向量数据库</h2>
			</div>
			<div>
				<label for="qdrant-url" class="text-xs text-ink-400 mb-1 block">Qdrant URL</label>
				<Input id="qdrant-url" bind:value={config.qdrant_url} class="bg-ink-800/50 border-ink-700/60 text-ink-100 max-w-md" />
			</div>
		</section>

		<!-- Save -->
		<div class="flex justify-end">
			<Button onclick={saveConfig} disabled={saving} class="bg-accent-500 hover:bg-accent-600 text-ink-950">
				{saving ? '保存中...' : '保存配置'}
			</Button>
		</div>
	{/if}
</div>
