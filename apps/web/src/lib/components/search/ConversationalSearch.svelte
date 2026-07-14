<script lang="ts">
	import { MessageSquare, Send, RotateCcw, Sparkles, BookOpen, Loader2 } from 'lucide-svelte';

	interface SearchMessage {
		id: string;
		role: 'user' | 'assistant';
		content: string;
		results?: Array<{
			book_title: string;
			chapter_title?: string;
			content_snippet: string;
			score: number;
		}>;
		timestamp: number;
	}

	interface Props {
		onSearch?: (query: string, context: SearchMessage[]) => Promise<{
			answer: string;
			results?: SearchMessage['results'];
		}>;
	}

	let { onSearch }: Props = $props();

	let messages = $state<SearchMessage[]>([]);
	let inputValue = $state('');
	let loading = $state(false);
	let messagesContainer = $state<HTMLElement | null>(null);

	const SUGGESTIONS = [
		'这本书的主角有什么能力?',
		'帮我找关于修炼体系的描述',
		'哪些章节提到了反派的动机?',
		'总结一下前50章的主线剧情',
	];

	async function handleSubmit() {
		const query = inputValue.trim();
		if (!query || loading) return;

		const userMsg: SearchMessage = {
			id: crypto.randomUUID(),
			role: 'user',
			content: query,
			timestamp: Date.now(),
		};
		messages = [...messages, userMsg];
		inputValue = '';
		loading = true;

		// Scroll to bottom
		requestAnimationFrame(() => {
			messagesContainer?.scrollTo({ top: messagesContainer.scrollHeight, behavior: 'smooth' });
		});

		try {
			if (onSearch) {
				const response = await onSearch(query, messages);
				const assistantMsg: SearchMessage = {
					id: crypto.randomUUID(),
					role: 'assistant',
					content: response.answer,
					results: response.results,
					timestamp: Date.now(),
				};
				messages = [...messages, assistantMsg];
			} else {
				// Fallback mock response
				const assistantMsg: SearchMessage = {
					id: crypto.randomUUID(),
					role: 'assistant',
					content: `关于「${query}」，我在您的书库中找到了一些相关内容。正在分析上下文以提供更精准的回答...`,
					timestamp: Date.now(),
				};
				messages = [...messages, assistantMsg];
			}
		} catch {
			const errorMsg: SearchMessage = {
				id: crypto.randomUUID(),
				role: 'assistant',
				content: '抱歉，搜索过程中出现了错误。请稍后再试。',
				timestamp: Date.now(),
			};
			messages = [...messages, errorMsg];
		} finally {
			loading = false;
			requestAnimationFrame(() => {
				messagesContainer?.scrollTo({ top: messagesContainer.scrollHeight, behavior: 'smooth' });
			});
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			handleSubmit();
		}
	}

	function clearConversation() {
		messages = [];
	}

	function useSuggestion(suggestion: string) {
		inputValue = suggestion;
		handleSubmit();
	}
</script>

<div class="flex h-full flex-col rounded-xl border border-ink-100 bg-white dark:border-ink-700 dark:bg-ink-900">
	<!-- Header -->
	<div class="flex items-center justify-between border-b border-ink-100 px-4 py-3 dark:border-ink-700">
		<div class="flex items-center gap-2">
			<MessageSquare class="h-4 w-4 text-accent-500" />
			<h3 class="text-sm font-semibold text-ink-800 dark:text-ink-200">对话式搜索</h3>
		</div>
		{#if messages.length > 0}
			<button
				class="flex items-center gap-1 rounded-md px-2 py-1 text-xs text-ink-400 transition-colors hover:bg-ink-100 hover:text-ink-600 dark:hover:bg-ink-800"
				onclick={clearConversation}
			>
				<RotateCcw class="h-3 w-3" />
				清除对话
			</button>
		{/if}
	</div>

	<!-- Messages area -->
	<div
		bind:this={messagesContainer}
		class="flex-1 overflow-y-auto p-4"
	>
		{#if messages.length === 0}
			<!-- Empty state with suggestions -->
			<div class="flex h-full flex-col items-center justify-center">
				<Sparkles class="mb-4 h-10 w-10 text-accent-300 opacity-50" />
				<p class="mb-4 text-sm text-ink-400">用自然语言搜索您的书库</p>
				<div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
					{#each SUGGESTIONS as suggestion}
						<button
							class="rounded-lg border border-ink-100 px-3 py-2 text-left text-xs text-ink-500 transition-all hover:border-accent-200 hover:bg-accent-50 dark:border-ink-700 dark:hover:border-accent-700 dark:hover:bg-accent-900/20"
							onclick={() => useSuggestion(suggestion)}
						>
							{suggestion}
						</button>
					{/each}
				</div>
			</div>
		{:else}
			<div class="space-y-4">
				{#each messages as msg (msg.id)}
					<div class="flex gap-3 {msg.role === 'user' ? 'justify-end' : ''}">
						{#if msg.role === 'assistant'}
							<div class="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full bg-accent-100 dark:bg-accent-900/30">
								<Sparkles class="h-3.5 w-3.5 text-accent-600 dark:text-accent-400" />
							</div>
						{/if}
						<div class="max-w-[80%] {msg.role === 'user'
							? 'rounded-2xl rounded-tr-md bg-accent-500 px-4 py-2 text-white'
							: 'rounded-2xl rounded-tl-md bg-ink-50 px-4 py-3 dark:bg-ink-800'
						}">
							<p class="text-sm whitespace-pre-wrap {msg.role === 'assistant' ? 'text-ink-700 dark:text-ink-300' : ''}">
								{msg.content}
							</p>

							<!-- Search results within message -->
							{#if msg.results && msg.results.length > 0}
								<div class="mt-3 space-y-2 border-t border-ink-100 pt-3 dark:border-ink-700">
									{#each msg.results.slice(0, 3) as result}
										<div class="rounded-lg bg-white/50 p-2 dark:bg-ink-900/50">
											<div class="flex items-center gap-1.5 text-xs text-ink-500">
												<BookOpen class="h-3 w-3" />
												<span class="font-medium">{result.book_title}</span>
												{#if result.chapter_title}
													<span>· {result.chapter_title}</span>
												{/if}
											</div>
											<p class="mt-1 line-clamp-2 text-xs text-ink-400">
												{result.content_snippet}
											</p>
										</div>
									{/each}
								</div>
							{/if}
						</div>
					</div>
				{/each}

				{#if loading}
					<div class="flex gap-3">
						<div class="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full bg-accent-100 dark:bg-accent-900/30">
							<Loader2 class="h-3.5 w-3.5 animate-spin text-accent-600 dark:text-accent-400" />
						</div>
						<div class="rounded-2xl rounded-tl-md bg-ink-50 px-4 py-3 dark:bg-ink-800">
							<div class="flex gap-1">
								<div class="h-2 w-2 animate-bounce rounded-full bg-ink-300" style="animation-delay: 0ms"></div>
								<div class="h-2 w-2 animate-bounce rounded-full bg-ink-300" style="animation-delay: 150ms"></div>
								<div class="h-2 w-2 animate-bounce rounded-full bg-ink-300" style="animation-delay: 300ms"></div>
							</div>
						</div>
					</div>
				{/if}
			</div>
		{/if}
	</div>

	<!-- Input area -->
	<div class="border-t border-ink-100 p-3 dark:border-ink-700">
		<div class="flex items-end gap-2">
			<textarea
				bind:value={inputValue}
				onkeydown={handleKeydown}
				placeholder="输入问题搜索书库..."
				rows={1}
				class="flex-1 resize-none rounded-xl border border-ink-200 bg-ink-50 px-4 py-2.5 text-sm text-ink-700 placeholder-ink-400 outline-none transition-colors focus:border-accent-300 focus:bg-white dark:border-ink-600 dark:bg-ink-800 dark:text-ink-300 dark:focus:border-accent-600 dark:focus:bg-ink-900"
			></textarea>
			<button
				class="flex h-10 w-10 items-center justify-center rounded-xl bg-accent-500 text-white transition-colors hover:bg-accent-600 disabled:opacity-50"
				onclick={handleSubmit}
				disabled={!inputValue.trim() || loading}
			>
				<Send class="h-4 w-4" />
			</button>
		</div>
		<p class="mt-1.5 text-center text-xs text-ink-300">
			支持多轮对话 · 搜索结果基于您的书库
		</p>
	</div>
</div>
