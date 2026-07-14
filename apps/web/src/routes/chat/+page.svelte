<script lang="ts">
	import { Chat } from '@ai-sdk/svelte';
	import { DefaultChatTransport } from 'ai';
	import { tick } from 'svelte';
	import { Send, Bot, User, Sparkles, BookOpen, Trash2, RotateCcw, Square } from 'lucide-svelte';
	import { useBooks } from '$lib/queries';

	let bookId = $state<string | null>(null);
	let includeRag = $state(true);
	let messagesContainer: HTMLDivElement;
	let inputRef: HTMLTextAreaElement;

	const booksQuery = useBooks(() => ({ per_page: 200 }));
	let books = $derived((booksQuery.data?.data ?? []).map(b => ({ id: b.id, title: b.title })));

	const chat = new Chat({
		transport: new DefaultChatTransport({
			api: '/api/chat',
			credentials: 'include',
			body: () => ({ bookId, includeRag }),
		}),
		onFinish() {
			scrollToBottom();
		},
		onError(event) {
			console.error('Chat error:', event);
		},
	});

	function handleSubmit() {
		const text = inputRef?.value?.trim();
		if (!text || chat.status === 'streaming' || chat.status === 'submitted') return;

		chat.sendMessage({ text });
		if (inputRef) inputRef.value = '';
		scrollToBottom();
	}

	function clearChat() {
		chat.messages = [];
	}

	async function scrollToBottom() {
		await tick();
		if (messagesContainer) {
			messagesContainer.scrollTop = messagesContainer.scrollHeight;
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			handleSubmit();
		}
	}

	// Auto-scroll on new messages
	$effect(() => {
		if (chat.messages.length > 0) {
			scrollToBottom();
		}
	});

	const isLoading = $derived(chat.status === 'streaming' || chat.status === 'submitted');
</script>

<svelte:head>
	<title>Nova Reader — AI 对话</title>
</svelte:head>

<div class="flex h-[calc(100vh-4rem)] flex-col">
	<!-- Header -->
	<div class="flex items-center justify-between border-b border-ink-800/50 px-6 py-3">
		<div class="flex items-center gap-3">
			<Bot size={20} class="text-accent-400" />
			<h1 class="text-lg font-semibold text-ink-100">AI 阅读助手</h1>
			{#if chat.status === 'streaming'}
				<span class="text-xs text-accent-400/70 animate-pulse">思考中...</span>
			{/if}
		</div>
		<div class="flex items-center gap-3">
			<!-- Book scope selector -->
			<select
				bind:value={bookId}
				class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-1.5 text-xs text-ink-300 focus:border-accent-500/50 focus:outline-none"
			>
				<option value={null}>全部书籍</option>
				{#each books as book}
					<option value={book.id}>{book.title}</option>
				{/each}
			</select>

			<!-- RAG toggle -->
			<label class="flex items-center gap-1.5 text-xs text-ink-400 cursor-pointer">
				<input
					type="checkbox"
					bind:checked={includeRag}
					class="rounded border-ink-600 bg-ink-800 text-accent-500 focus:ring-accent-500/30"
				/>
				<Sparkles size={12} />
				RAG 检索
			</label>

			<!-- Clear -->
			<button
				onclick={clearChat}
				class="rounded-lg p-1.5 text-ink-500 hover:bg-ink-800/50 hover:text-ink-300 transition-colors"
				title="清空对话"
			>
				<Trash2 size={16} />
			</button>
		</div>
	</div>

	<!-- Messages -->
	<div bind:this={messagesContainer} class="flex-1 overflow-y-auto px-6 py-4 space-y-4">
		{#if chat.messages.length === 0}
			<div class="flex h-full items-center justify-center">
				<div class="text-center space-y-3">
					<BookOpen size={48} class="mx-auto text-ink-700" strokeWidth={1} />
					<p class="text-ink-500">向 AI 提问关于你的书籍的任何问题</p>
					<div class="flex flex-wrap justify-center gap-2">
						{#each ['这本书的主题是什么？', '总结第一章', '主角的性格特点', '分析写作风格'] as suggestion}
							<button
								onclick={() => {
									chat.sendMessage({ text: suggestion });
								}}
								class="rounded-lg border border-ink-700/50 bg-ink-900/30 px-3 py-1.5 text-xs text-ink-400 hover:border-accent-500/30 hover:text-ink-200 transition-colors"
							>{suggestion}</button>
						{/each}
					</div>
				</div>
			</div>
		{:else}
			{#each chat.messages as message (message.id)}
				<div class="flex gap-3 {message.role === 'user' ? 'justify-end' : ''}">
					{#if message.role === 'assistant'}
						<div class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-accent-500/10">
							<Bot size={14} class="text-accent-400" />
						</div>
					{/if}

					<div class="max-w-[75%] {message.role === 'user' ? 'bg-accent-500/10 border-accent-500/20' : 'bg-ink-900/30 border-ink-800/50'} rounded-xl border px-4 py-3">
						{#each message.parts as part}
							{#if part.type === 'text'}
								<div class="text-sm text-ink-200 whitespace-pre-wrap prose prose-invert prose-sm max-w-none">
									{part.text}
								</div>
							{/if}
						{/each}
					</div>

					{#if message.role === 'user'}
						<div class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-ink-800/50">
							<User size={14} class="text-ink-400" />
						</div>
					{/if}
				</div>
			{/each}

			{#if chat.status === 'submitted'}
				<div class="flex gap-3">
					<div class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-accent-500/10">
						<Bot size={14} class="text-accent-400 animate-pulse" />
					</div>
					<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 px-4 py-3">
						<span class="inline-flex gap-1">
							<span class="h-1.5 w-1.5 rounded-full bg-ink-500 animate-bounce" style="animation-delay: 0ms"></span>
							<span class="h-1.5 w-1.5 rounded-full bg-ink-500 animate-bounce" style="animation-delay: 150ms"></span>
							<span class="h-1.5 w-1.5 rounded-full bg-ink-500 animate-bounce" style="animation-delay: 300ms"></span>
						</span>
					</div>
				</div>
			{/if}
		{/if}
	</div>

	<!-- Input -->
	<div class="border-t border-ink-800/50 px-6 py-4">
		<div class="flex items-end gap-3">
			<textarea
				bind:this={inputRef}
				onkeydown={handleKeydown}
				placeholder="输入你的问题..."
				rows="1"
				disabled={isLoading}
				class="flex-1 resize-none rounded-xl border border-ink-700/50 bg-ink-900/30 px-4 py-3 text-sm text-ink-200 placeholder:text-ink-600 focus:border-accent-500/50 focus:outline-none focus:ring-1 focus:ring-accent-500/20 disabled:opacity-50"
			></textarea>
			{#if isLoading}
				<button
					onclick={() => chat.stop()}
					class="flex h-11 w-11 items-center justify-center rounded-xl bg-red-500/80 text-white transition-all hover:bg-red-500"
					title="停止生成"
				>
					<Square size={14} />
				</button>
			{:else}
				<button
					onclick={handleSubmit}
					class="flex h-11 w-11 items-center justify-center rounded-xl bg-accent-500 text-white transition-all hover:bg-accent-400 disabled:opacity-30 disabled:cursor-not-allowed"
				>
					<Send size={16} />
				</button>
			{/if}
		</div>
		{#if chat.error}
			<div class="mt-2 flex items-center gap-2 text-xs text-red-400">
				<span>请求失败: {chat.error.message}</span>
				<button
					onclick={() => {
						chat.clearError();
						chat.regenerate();
					}}
					class="flex items-center gap-1 rounded px-2 py-0.5 hover:bg-red-500/10"
				>
					<RotateCcw size={12} />
					重试
				</button>
			</div>
		{/if}
	</div>
</div>
