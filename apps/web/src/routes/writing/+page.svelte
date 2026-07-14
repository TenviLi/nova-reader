<script lang="ts">
import { getErrorMessage } from '$lib/utils';
	import { api } from '$services/api';
	import { toast } from 'svelte-sonner';
	import { Menu, BarChart3, Eraser, FileText, Lightbulb, Network, Plus, Send, Tags } from 'lucide-svelte';
	import { browser } from '$app/environment';
	import MarkdownRenderer from '$lib/components/ui/MarkdownRenderer.svelte';
	import { featureFlags } from '$stores/settings.svelte';

	let activeProject = $state<string | null>(null);
	let editorContent = $state(browser ? localStorage.getItem('nova_writing_draft') ?? '' : '');
	let aiChatVisible = $state(true);
	let outlineVisible = $state(true);
	let wordCount = $derived(editorContent.replace(/\s/g, '').length);
	let saving = $state(false);
	let aiEnabled = $derived(featureFlags.isEnabled('ai_chat'));

	// Auto-save editor content to localStorage (debounced)
	let autoSaveTimer: ReturnType<typeof setTimeout> | null = null;
	$effect(() => {
		const content = editorContent;
		if (!browser) return;
		if (autoSaveTimer) clearTimeout(autoSaveTimer);
		autoSaveTimer = setTimeout(() => {
			localStorage.setItem('nova_writing_draft', content);
		}, 1000);
	});

	function saveContent() {
		if (!browser) return;
		saving = true;
		localStorage.setItem('nova_writing_draft', editorContent);
		toast.success('草稿已保存');
		saving = false;
	}

	let chatMessages = $state<Array<{
		role: 'user' | 'assistant';
		content: string;
		timestamp: number;
	}>>([]);
	let chatInput = $state('');
	let generating = $state(false);
	let streamingContent = $state('');

	let outline = $state.raw<Array<{
		id: string;
		title: string;
		summary: string;
		status: 'draft' | 'in-progress' | 'done';
		word_count: number;
	}>>([]);

	// AI Chat with streaming
	function addAssistantMessage(content: string) {
		chatMessages = [...chatMessages, {
			role: 'assistant',
			content,
			timestamp: Date.now(),
		}];
	}

	async function sendChat() {
		const userMsg = chatInput.trim();
		if (!userMsg || generating) return;

		chatMessages = [...chatMessages, { role: 'user', content: userMsg, timestamp: Date.now() }];
		chatInput = '';
		generating = true;
		streamingContent = '';

		try {
			// Build context with current editor content
			const systemPrompt = editorContent
				? `你是一个小说创作AI助手。用户正在写作，当前内容如下：\n\n---\n${editorContent.slice(-2000)}\n---\n\n请根据上下文回答用户的问题或执行创作指令。`
				: '你是一个小说创作AI助手，帮助用户构思、续写、修改文本。';

			const messages = [
				{ role: 'system', content: systemPrompt },
				...chatMessages.slice(-10).map(m => ({ role: m.role, content: m.content })),
			];

			await api.streamAiChat(
				messages,
				(token) => {
					streamingContent += token;
				},
				{ temperature: 0.7 }
			);

			chatMessages = [...chatMessages, {
				role: 'assistant',
				content: streamingContent,
				timestamp: Date.now(),
			}];
		} catch (e: unknown) {
			chatMessages = [...chatMessages, {
				role: 'assistant',
				content: `⚠️ 错误: ${getErrorMessage(e)}`,
				timestamp: Date.now(),
			}];
		} finally {
			generating = false;
			streamingContent = '';
		}
	}

	async function summarizeDraft() {
		if (!editorContent.trim()) {
			toast.error('请先输入需要总结的文本');
			return;
		}
		generating = true;
		try {
			const result = await api.aiSummarize(editorContent.slice(0, 12000), 'bullet_points');
			addAssistantMessage(`## 摘要\n\n${result.summary || '暂无摘要'}\n\n${result.key_points.length ? `### 关键点\n${result.key_points.map(point => `- ${point}`).join('\n')}` : ''}`);
			toast.success('摘要已生成');
		} catch (e: unknown) {
			toast.error(getErrorMessage(e));
		} finally {
			generating = false;
		}
	}

	async function extractEntities() {
		if (!editorContent.trim()) {
			toast.error('请先输入需要识别的文本');
			return;
		}
		generating = true;
		try {
			const result = await api.aiExtractEntities(editorContent.slice(0, 12000));
			const entityLines = result.entities.map(entity => {
				const aliases = entity.aliases.length ? `（${entity.aliases.join(' / ')}）` : '';
				return `- **${entity.name}** ${aliases} · ${entity.entity_type}: ${entity.description || '暂无描述'}`;
			});
			const relationLines = result.relationships.map(rel =>
				`- ${rel.source} → ${rel.target} · ${rel.relationship_type}: ${rel.description || '暂无描述'}`
			);
			addAssistantMessage(`## 实体识别\n\n${entityLines.length ? entityLines.join('\n') : '未识别到实体。'}\n\n${relationLines.length ? `### 关系\n${relationLines.join('\n')}` : ''}`);
			toast.success(`识别到 ${result.entities.length} 个实体`);
		} catch (e: unknown) {
			toast.error(getErrorMessage(e));
		} finally {
			generating = false;
		}
	}

	async function suggestTags() {
		if (!editorContent.trim()) {
			toast.error('请先输入作品简介或正文片段');
			return;
		}
		const firstLine = editorContent.split('\n').map(line => line.trim()).find(Boolean);
		generating = true;
		try {
			const result = await api.aiSuggestTags(
				firstLine?.slice(0, 80) || '未命名作品',
				editorContent.slice(0, 800),
				editorContent.slice(0, 3000)
			);
			addAssistantMessage([
				'## 标签建议',
				'',
				`**类型**：${result.genres.length ? result.genres.join('、') : '暂无'}`,
				`**标签**：${result.tags.length ? result.tags.join('、') : '暂无'}`,
				`**主题**：${result.themes.length ? result.themes.join('、') : '暂无'}`,
			].join('\n'));
			toast.success('标签建议已生成');
		} catch (e: unknown) {
			toast.error(getErrorMessage(e));
		} finally {
			generating = false;
		}
	}

	async function cleanupForumDraft() {
		if (!editorContent.trim()) {
			toast.error('请先粘贴需要清理的论坛文本');
			return;
		}
		generating = true;
		try {
			const result = await api.cleanupForumText(editorContent);
			editorContent = result.cleaned_text;
			addAssistantMessage(`## 文本清理完成\n\n已尝试移除 ${result.removed_count} 行论坛噪声，编辑器已替换为清理后的文本。`);
			toast.success('文本已清理');
		} catch (e: unknown) {
			toast.error(getErrorMessage(e));
		} finally {
			generating = false;
		}
	}

	async function generateOutline() {
		if (!editorContent.trim()) {
			toast.error('请先输入故事设定或前言');
			return;
		}
		generating = true;
		try {
			const result = await api.aiGenerateOutline(editorContent.slice(0, 3000), undefined, 10);
			outline = result.chapters.map((ch, i) => ({
				id: `ch-${i}`,
				title: ch.title,
				summary: ch.summary,
				status: 'draft' as const,
				word_count: 0,
			}));
			toast.success(`已生成 ${result.chapters.length} 章大纲`);
		} catch (e: unknown) {
			toast.error(getErrorMessage(e));
		} finally {
			generating = false;
		}
	}

	async function analyzeStyle() {
		if (!editorContent.trim()) return;
		generating = true;
		try {
			const result = await api.aiAnalyzeStyle(editorContent.slice(0, 5000));
			chatMessages = [...chatMessages, {
				role: 'assistant',
				content: `📝 **风格分析报告**\n\n• 语气: ${result.tone}\n• 视角: ${result.pov}\n• 句长: ${result.avg_sentence_length}\n• 词汇丰富度: ${(result.vocabulary_richness * 100).toFixed(0)}%\n• 对话占比: ${(result.dialogue_ratio * 100).toFixed(0)}%\n• 节奏: ${result.pacing}\n\n💡 建议:\n${result.suggestions.map(s => `• ${s}`).join('\n')}`,
				timestamp: Date.now(),
			}];
		} catch (e: unknown) {
			toast.error(getErrorMessage(e));
		} finally {
			generating = false;
		}
	}

	function insertToEditor(text: string) {
		editorContent += '\n\n' + text;
		toast.success('已插入到编辑器');
	}

	function handleChatKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			sendChat();
		}
	}
</script>

<svelte:head>
	<title>Nova Reader — 创作工坊</title>
</svelte:head>

<div class="flex h-[calc(100vh-4rem)] overflow-hidden animate-fade-in">
	<!-- Outline sidebar -->
	{#if outlineVisible}
		<aside class="w-64 shrink-0 flex flex-col border-r border-ink-800/50 bg-ink-950">
			<div class="border-b border-ink-800/50 px-4 py-3 flex items-center justify-between">
				<h3 class="text-sm font-semibold text-ink-100">大纲</h3>
				<button
					onclick={generateOutline}
					disabled={generating}
					class="text-xs text-accent-400 hover:text-accent-300 disabled:opacity-50"
				>AI 生成</button>
			</div>
			<div class="flex-1 overflow-y-auto p-2">
				{#if outline.length === 0}
					<div class="py-8 text-center text-xs text-ink-500">
						<p>开始规划你的故事</p>
						<p class="mt-1 text-ink-600">AI 可以帮你生成大纲</p>
					</div>
				{:else}
					{#each outline as chapter, i}
						<div class="mb-1 rounded-lg px-3 py-2 hover:bg-ink-800/50 cursor-pointer transition-colors">
							<div class="flex items-center gap-2">
								<span class="text-[10px] text-ink-500 w-4">{i + 1}</span>
								<span class="flex-1 text-xs text-ink-200 truncate">{chapter.title}</span>
								<span
									class="h-2 w-2 rounded-full"
									class:bg-accent-400={chapter.status === 'done'}
									class:bg-amber-400={chapter.status === 'in-progress'}
									class:bg-ink-600={chapter.status === 'draft'}
								></span>
							</div>
						</div>
					{/each}
				{/if}
			</div>
		</aside>
	{/if}

	<!-- Main editor -->
	<div class="flex flex-1 flex-col">
		<!-- Editor toolbar -->
		<div class="flex items-center justify-between border-b border-ink-800/50 px-4 py-2">
			<div class="flex items-center gap-3">
					<button
						type="button"
						onclick={() => outlineVisible = !outlineVisible}
						aria-label={outlineVisible ? '隐藏大纲' : '显示大纲'}
						class="rounded-md p-1.5 text-ink-400 transition-colors hover:text-ink-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
					>
						<Menu size={16} strokeWidth={2} />
					</button>
				<span class="text-xs text-ink-500">{wordCount} 字</span>
			</div>
			<div class="flex flex-wrap items-center justify-end gap-2">
					<button
						type="button"
						onclick={summarizeDraft}
						disabled={generating || !editorContent.trim()}
					aria-label="总结当前草稿"
					class="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs text-ink-400 hover:text-accent-400 hover:bg-accent-500/10 transition-colors disabled:opacity-50"
				>
					<FileText size={14} strokeWidth={2} />
					摘要
				</button>
					<button
						type="button"
						onclick={extractEntities}
					disabled={generating || !editorContent.trim()}
					aria-label="识别当前草稿实体"
					class="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs text-ink-400 hover:text-accent-400 hover:bg-accent-500/10 transition-colors disabled:opacity-50"
				>
					<Network size={14} strokeWidth={2} />
					实体
				</button>
					<button
						type="button"
						onclick={suggestTags}
					disabled={generating || !editorContent.trim()}
					aria-label="建议当前草稿标签"
					class="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs text-ink-400 hover:text-accent-400 hover:bg-accent-500/10 transition-colors disabled:opacity-50"
				>
					<Tags size={14} strokeWidth={2} />
					标签
				</button>
					<button
						type="button"
						onclick={cleanupForumDraft}
					disabled={generating || !editorContent.trim()}
					aria-label="清理论坛文本"
					class="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs text-ink-400 hover:text-accent-400 hover:bg-accent-500/10 transition-colors disabled:opacity-50"
				>
					<Eraser size={14} strokeWidth={2} />
					清理
				</button>
					<button
						type="button"
						onclick={analyzeStyle}
						disabled={generating || !editorContent.trim()}
						aria-label="分析当前草稿风格"
						class="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs text-ink-400 hover:text-accent-400 hover:bg-accent-500/10 transition-colors disabled:opacity-50"
					>
					<BarChart3 size={14} strokeWidth={2} />
					风格分析
				</button>
					<button
						type="button"
						onclick={() => aiChatVisible = !aiChatVisible}
						aria-pressed={aiChatVisible}
						class="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs transition-colors {aiChatVisible ? 'bg-accent-500/10' : ''}"
					class:text-accent-400={aiChatVisible}
					class:text-ink-400={!aiChatVisible}
				>
					<Lightbulb size={14} strokeWidth={2} />
					AI 助手
				</button>
					<button
						type="button"
						onclick={saveContent}
					disabled={saving}
					class="rounded-lg bg-accent-500 px-4 py-1.5 text-xs font-medium text-ink-950 hover:bg-accent-400 transition-colors disabled:opacity-50"
				>
					{saving ? '保存中...' : '保存'}
				</button>
			</div>
		</div>

		<!-- Editor area -->
		<div class="flex-1 overflow-y-auto">
			<div class="mx-auto max-w-3xl p-8">
				<textarea
					bind:value={editorContent}
					placeholder="开始创作..."
					class="w-full min-h-[60vh] resize-none bg-transparent text-ink-200 placeholder-ink-600 outline-none text-base leading-[1.8]"
					style="font-family: var(--font-serif);"
				></textarea>
			</div>
		</div>
	</div>

	<!-- AI Chat panel -->
	{#if aiChatVisible && aiEnabled}
		<aside class="w-80 shrink-0 flex flex-col border-l border-ink-800/50 bg-ink-950">
			<div class="border-b border-ink-800/50 px-4 py-3">
				<h3 class="text-sm font-semibold text-ink-100">AI 创作助手</h3>
				<p class="text-[10px] text-ink-500 mt-0.5">基于你的书库知识进行辅助</p>
			</div>

			<!-- Chat messages -->
			<div class="flex-1 overflow-y-auto p-3 space-y-3">
				{#if chatMessages.length === 0}
					<div class="py-8 text-center">
						<p class="text-sm text-ink-500">试试问我：</p>
						<div class="mt-3 space-y-2">
							{#each ['帮我续写这段', '修改这段的语气', '生成下一章大纲', '分析角色动机', '检查情节冲突'] as suggestion}
								<button
									onclick={() => { chatInput = suggestion; sendChat(); }}
									class="block w-full rounded-lg border border-ink-800/50 bg-ink-900/30 px-3 py-2 text-xs text-ink-300 text-left hover:border-ink-700/50 hover:text-accent-400 transition-colors"
								>
									{suggestion}
								</button>
							{/each}
						</div>
					</div>
				{:else}
					{#each chatMessages as msg}
						<div
							class="rounded-lg px-3 py-2 text-sm {msg.role === 'user' ? 'bg-accent-500/10' : ''} {msg.role === 'assistant' ? 'bg-ink-800/30' : ''}"
							class:text-ink-200={msg.role === 'user'}
							class:text-ink-300={msg.role === 'assistant'}
						>
							<div class="flex items-start justify-between gap-2">
								{#if msg.role === 'assistant'}
									<MarkdownRenderer content={msg.content} class="flex-1" />
								{:else}
									<span class="flex-1 whitespace-pre-wrap">{msg.content}</span>
								{/if}
								{#if msg.role === 'assistant' && msg.content.length > 20}
										<button
											type="button"
											onclick={() => insertToEditor(msg.content)}
											aria-label="将助手回复插入编辑器"
											class="shrink-0 mt-0.5 rounded p-1 text-ink-500 hover:text-accent-400 hover:bg-accent-500/10 transition-colors"
											title="插入到编辑器"
									>
										<Plus size={12} strokeWidth={2} />
									</button>
								{/if}
							</div>
						</div>
					{/each}
					{#if generating && streamingContent}
						<div class="rounded-lg bg-ink-800/30 px-3 py-2 text-sm text-ink-300">
							<MarkdownRenderer content={streamingContent} /><span class="inline-block w-1.5 h-4 bg-accent-400 animate-pulse ml-0.5"></span>
						</div>
					{:else if generating}
						<div class="flex items-center gap-2 px-3 py-2 text-sm text-ink-400">
							<div class="flex gap-1">
								<div class="h-1.5 w-1.5 animate-bounce rounded-full bg-accent-400" style="animation-delay: 0ms"></div>
								<div class="h-1.5 w-1.5 animate-bounce rounded-full bg-accent-400" style="animation-delay: 150ms"></div>
								<div class="h-1.5 w-1.5 animate-bounce rounded-full bg-accent-400" style="animation-delay: 300ms"></div>
							</div>
						</div>
					{/if}
				{/if}
			</div>

			<!-- Chat input -->
			<div class="border-t border-ink-800/50 p-3">
				<div class="flex items-end gap-2">
					<textarea
						bind:value={chatInput}
						placeholder="输入指令... (Enter 发送, Shift+Enter 换行)"
						rows="2"
						onkeydown={handleChatKeydown}
						class="flex-1 resize-none rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-2 text-sm text-ink-200 placeholder-ink-500 outline-none focus:border-accent-500/30"
					></textarea>
					<button
						type="button"
						onclick={sendChat}
						disabled={!chatInput.trim() || generating}
						aria-label="发送给 AI 创作助手"
						class="shrink-0 rounded-lg bg-accent-500 p-2.5 text-ink-950 hover:bg-accent-400 transition-colors disabled:opacity-50"
					>
						<Send size={16} strokeWidth={2} />
					</button>
				</div>
			</div>
		</aside>
	{/if}
</div>
