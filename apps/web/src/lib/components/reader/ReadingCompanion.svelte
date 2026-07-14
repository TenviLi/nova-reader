<script lang="ts">
	import { Send, Bot, User, Loader2, BookOpen, AlertTriangle } from 'lucide-svelte';
	import { api } from '$lib/services/api';
	import { featureFlags } from '$stores/settings.svelte';

	interface Props {
		bookId: string;
		bookTitle: string;
		currentChapter?: number;
		totalChapters?: number;
	}

	let { bookId, bookTitle, currentChapter = 0, totalChapters = 0 }: Props = $props();

	// Check if AI chat is enabled
	let aiEnabled = $derived(featureFlags.isEnabled('ai_chat'));

	interface Message {
		role: 'user' | 'assistant';
		content: string;
		timestamp: Date;
		spoilerWarning?: boolean;
	}

	let messages = $state<Message[]>([]);
	let input = $state('');
	let loading = $state(false);
	let spoilerProtection = $state(true);
	let chatContainer: HTMLDivElement | undefined = $state();

	const QUICK_PROMPTS = [
		{ label: '总结当前章节', prompt: '请总结当前章节的主要内容' },
		{ label: '解释角色关系', prompt: '请解释当前出现的角色之间的关系' },
		{ label: '解释生词', prompt: '请解释这一章中出现的难懂词汇或典故' },
		{ label: '预测下文', prompt: '根据已有情节，你觉得接下来会怎样发展？' },
	];

	async function sendMessage() {
		const text = input.trim();
		if (!text || loading) return;

		messages = [...messages, { role: 'user', content: text, timestamp: new Date() }];
		input = '';
		loading = true;

		try {
			const systemPrompt = spoilerProtection
				? `You are a reading companion for "${bookTitle}". The user is currently at chapter ${currentChapter + 1} of ${totalChapters}. CRITICAL: Do NOT reveal any plot points, deaths, twists, or events that happen AFTER chapter ${currentChapter + 1}. If the user asks about future events, gently refuse and explain you want to protect their reading experience. Answer in the same language the user writes in.`
				: `You are a reading companion for "${bookTitle}". The user is at chapter ${currentChapter + 1} of ${totalChapters}. Answer freely about the entire book. Answer in the same language the user writes in.`;

			const response = await api.chatCompanion({
				book_id: bookId,
				message: text,
				system_prompt: systemPrompt,
				context: {
					current_chapter: currentChapter,
					total_chapters: totalChapters,
					spoiler_protection: spoilerProtection,
				},
			});

			const hasSpoilerWarning = response.content?.includes('[剧透警告]') ||
				response.content?.includes('[spoiler]');

			messages = [...messages, {
				role: 'assistant',
				content: response.content || '抱歉，我暂时无法回答这个问题。',
				timestamp: new Date(),
				spoilerWarning: hasSpoilerWarning,
			}];
		} catch {
			messages = [...messages, {
				role: 'assistant',
				content: '⚠️ 连接失败，请稍后重试。',
				timestamp: new Date(),
			}];
		} finally {
			loading = false;
			// Scroll to bottom
			requestAnimationFrame(() => {
				chatContainer?.scrollTo({ top: chatContainer.scrollHeight, behavior: 'smooth' });
			});
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			sendMessage();
		}
	}

	function useQuickPrompt(prompt: string) {
		input = prompt;
		sendMessage();
	}
</script>

{#if !aiEnabled}
<div class="flex h-full flex-col items-center justify-center rounded-lg border border-ink-200 bg-white p-6 dark:border-ink-700 dark:bg-ink-900">
	<Bot class="h-8 w-8 text-ink-400" />
	<p class="mt-2 text-sm text-ink-500">AI 对话功能已关闭</p>
	<p class="text-xs text-ink-400">请在管理面板中启用</p>
</div>
{:else}
<div class="flex h-full flex-col rounded-lg border border-ink-200 bg-white dark:border-ink-700 dark:bg-ink-900">
	<!-- Header -->
	<div class="flex items-center justify-between border-b border-ink-100 px-4 py-3 dark:border-ink-700">
		<div class="flex items-center gap-2">
			<Bot class="h-4 w-4 text-accent-500" />
			<span class="text-sm font-medium text-ink-800 dark:text-ink-200">阅读伴侣</span>
		</div>
		<label class="flex cursor-pointer items-center gap-1.5 text-xs text-ink-500">
			<input
				type="checkbox"
				bind:checked={spoilerProtection}
				class="h-3 w-3 rounded accent-accent-500"
			/>
			<AlertTriangle class="h-3 w-3" />
			防剧透
		</label>
	</div>

	<!-- Messages -->
	<div
		bind:this={chatContainer}
		class="flex-1 space-y-3 overflow-y-auto p-4"
	>
		{#if messages.length === 0}
			<!-- Empty state with quick prompts -->
			<div class="flex h-full flex-col items-center justify-center gap-4 text-center">
				<BookOpen class="h-8 w-8 text-ink-300" />
				<div class="space-y-1">
					<p class="text-sm font-medium text-ink-600 dark:text-ink-400">
						我是你的阅读伴侣
					</p>
					<p class="text-xs text-ink-400">
						关于《{bookTitle}》的任何问题都可以问我
					</p>
				</div>
				<div class="flex flex-wrap justify-center gap-2">
					{#each QUICK_PROMPTS as { label, prompt }}
						<button
							class="rounded-full border border-ink-200 px-3 py-1 text-xs text-ink-600 transition-colors hover:border-accent-300 hover:bg-accent-50 hover:text-accent-700 dark:border-ink-600 dark:text-ink-400 dark:hover:border-accent-600 dark:hover:bg-accent-900/20"
							onclick={() => useQuickPrompt(prompt)}
						>
							{label}
						</button>
					{/each}
				</div>
			</div>
		{:else}
			{#each messages as message}
				<div class="flex gap-2 {message.role === 'user' ? 'justify-end' : ''}">
					{#if message.role === 'assistant'}
						<div class="flex-shrink-0 mt-0.5">
							<Bot class="h-5 w-5 text-accent-500" />
						</div>
					{/if}
					<div class="max-w-[85%] rounded-lg px-3 py-2 text-sm {message.role === 'user'
						? 'bg-accent-500 text-white'
						: 'bg-ink-50 text-ink-800 dark:bg-ink-800 dark:text-ink-200'
					}">
						{#if message.spoilerWarning}
							<div class="mb-1 flex items-center gap-1 text-xs text-amber-600 dark:text-amber-400">
								<AlertTriangle class="h-3 w-3" />
								<span>含有剧透内容</span>
							</div>
						{/if}
						<p class="whitespace-pre-wrap">{message.content}</p>
					</div>
					{#if message.role === 'user'}
						<div class="flex-shrink-0 mt-0.5">
							<User class="h-5 w-5 text-ink-400" />
						</div>
					{/if}
				</div>
			{/each}
			{#if loading}
				<div class="flex gap-2">
					<Bot class="h-5 w-5 flex-shrink-0 text-accent-500" />
					<div class="flex items-center gap-1 rounded-lg bg-ink-50 px-3 py-2 dark:bg-ink-800">
						<Loader2 class="h-3.5 w-3.5 animate-spin text-accent-500" />
						<span class="text-xs text-ink-500">思考中...</span>
					</div>
				</div>
			{/if}
		{/if}
	</div>

	<!-- Input -->
	<div class="border-t border-ink-100 p-3 dark:border-ink-700">
		<div class="flex items-end gap-2">
			<textarea
				class="flex-1 resize-none rounded-lg border border-ink-200 bg-ink-50 px-3 py-2 text-sm placeholder-ink-400 focus:border-accent-400 focus:outline-none dark:border-ink-600 dark:bg-ink-800 dark:placeholder-ink-500"
				rows={1}
				bind:value={input}
				onkeydown={handleKeydown}
				placeholder="问一个关于这本书的问题..."
				disabled={loading}
			></textarea>
			<button
				class="flex-shrink-0 rounded-lg bg-accent-500 p-2 text-white transition-colors hover:bg-accent-600 disabled:opacity-50"
				onclick={sendMessage}
				disabled={!input.trim() || loading}
			>
				<Send class="h-4 w-4" />
			</button>
		</div>
	</div>
</div>
{/if}
