<script lang="ts">
	import { Users, MessageCircle, Heart, Reply, MoreHorizontal, Send } from 'lucide-svelte';

	interface Annotation {
		id: string;
		user_id: string;
		username: string;
		avatar_color: string;
		chapter_index: number;
		start_offset: number;
		end_offset: number;
		highlighted_text: string;
		note: string;
		emoji_reaction?: string;
		replies: AnnotationReply[];
		created_at: string;
	}

	interface AnnotationReply {
		id: string;
		user_id: string;
		username: string;
		content: string;
		created_at: string;
	}

	interface Props {
		annotations: Annotation[];
		currentUserId: string;
		chapterIndex?: number;
		onAddAnnotation?: (text: string, note: string) => void;
		onReply?: (annotationId: string, content: string) => void;
		onReact?: (annotationId: string, emoji: string) => void;
	}

	let {
		annotations = [],
		currentUserId,
		chapterIndex,
		onAddAnnotation,
		onReply,
		onReact,
	}: Props = $props();

	let replyingTo = $state<string | null>(null);
	let replyText = $state('');

	let filteredAnnotations = $derived(
		chapterIndex !== undefined
			? annotations.filter(a => a.chapter_index === chapterIndex)
			: annotations
	);

	let sortedAnnotations = $derived(
		[...filteredAnnotations].sort((a, b) => a.start_offset - b.start_offset)
	);

	function submitReply(annotationId: string) {
		if (!replyText.trim()) return;
		onReply?.(annotationId, replyText.trim());
		replyText = '';
		replyingTo = null;
	}

	function getTimeAgo(dateStr: string): string {
		const diff = Date.now() - new Date(dateStr).getTime();
		const minutes = Math.floor(diff / 60000);
		if (minutes < 1) return '刚刚';
		if (minutes < 60) return `${minutes}分钟前`;
		const hours = Math.floor(minutes / 60);
		if (hours < 24) return `${hours}小时前`;
		const days = Math.floor(hours / 24);
		return `${days}天前`;
	}

	const AVATAR_COLORS = [
		'bg-blue-500', 'bg-purple-500', 'bg-emerald-500', 'bg-amber-500',
		'bg-pink-500', 'bg-cyan-500', 'bg-red-500', 'bg-indigo-500',
	];
</script>

<div class="flex flex-col rounded-xl border border-ink-100 bg-white dark:border-ink-700 dark:bg-ink-900">
	<!-- Header -->
	<div class="flex items-center justify-between border-b border-ink-100 px-4 py-3 dark:border-ink-700">
		<div class="flex items-center gap-2">
			<Users class="h-4 w-4 text-accent-500" />
			<h3 class="text-sm font-semibold text-ink-800 dark:text-ink-200">读书会批注</h3>
			<span class="rounded-full bg-ink-100 px-2 py-0.5 text-xs text-ink-500 dark:bg-ink-700">
				{sortedAnnotations.length}
			</span>
		</div>
	</div>

	<!-- Annotations list -->
	<div class="flex-1 overflow-y-auto p-4">
		{#if sortedAnnotations.length === 0}
			<div class="py-8 text-center text-ink-400">
				<MessageCircle class="mx-auto mb-3 h-10 w-10 opacity-30" />
				<p class="text-sm">还没有批注</p>
				<p class="mt-1 text-xs text-ink-300">选中文本即可添加批注，与其他读者交流</p>
			</div>
		{:else}
			<div class="space-y-4">
				{#each sortedAnnotations as annotation (annotation.id)}
					<div class="rounded-lg border border-ink-100 p-3 dark:border-ink-700">
						<!-- User info + timestamp -->
						<div class="mb-2 flex items-center gap-2">
							<div class="flex h-6 w-6 items-center justify-center rounded-full text-xs font-medium text-white {annotation.avatar_color}">
								{annotation.username.charAt(0).toUpperCase()}
							</div>
							<span class="text-sm font-medium text-ink-700 dark:text-ink-300">
								{annotation.username}
							</span>
							<span class="text-xs text-ink-400">
								{getTimeAgo(annotation.created_at)}
							</span>
						</div>

						<!-- Highlighted text -->
						<div class="mb-2 border-l-2 border-accent-300 bg-accent-50/50 px-3 py-1.5 dark:border-accent-700 dark:bg-accent-900/10">
							<p class="line-clamp-2 text-xs italic text-ink-500 dark:text-ink-400">
								"{annotation.highlighted_text}"
							</p>
						</div>

						<!-- Note -->
						<p class="mb-2 text-sm text-ink-700 dark:text-ink-300">
							{annotation.note}
						</p>

						<!-- Actions -->
						<div class="flex items-center gap-3">
							<button
								class="flex items-center gap-1 text-xs text-ink-400 hover:text-red-500"
								onclick={() => onReact?.(annotation.id, '❤️')}
							>
								<Heart class="h-3.5 w-3.5" />
								{#if annotation.emoji_reaction}
									<span>{annotation.emoji_reaction}</span>
								{/if}
							</button>
							<button
								class="flex items-center gap-1 text-xs text-ink-400 hover:text-accent-500"
								onclick={() => replyingTo = replyingTo === annotation.id ? null : annotation.id}
							>
								<Reply class="h-3.5 w-3.5" />
								{annotation.replies.length || ''}
							</button>
						</div>

						<!-- Replies -->
						{#if annotation.replies.length > 0}
							<div class="mt-3 space-y-2 border-t border-ink-50 pt-2 dark:border-ink-700">
								{#each annotation.replies as reply}
									<div class="flex gap-2 pl-4">
										<div class="h-5 w-5 flex-shrink-0 rounded-full bg-ink-200 dark:bg-ink-600"></div>
										<div>
											<span class="text-xs font-medium text-ink-600 dark:text-ink-400">{reply.username}</span>
											<p class="text-xs text-ink-500">{reply.content}</p>
										</div>
									</div>
								{/each}
							</div>
						{/if}

						<!-- Reply input -->
						{#if replyingTo === annotation.id}
							<div class="mt-2 flex gap-2 border-t border-ink-50 pt-2 dark:border-ink-700">
								<input
									type="text"
									bind:value={replyText}
									placeholder="回复..."
									class="flex-1 rounded-lg border border-ink-200 bg-ink-50 px-3 py-1.5 text-xs outline-none focus:border-accent-300 dark:border-ink-600 dark:bg-ink-800"
									onkeydown={(e) => e.key === 'Enter' && submitReply(annotation.id)}
								/>
								<button
									class="flex h-7 w-7 items-center justify-center rounded-lg bg-accent-500 text-white"
									onclick={() => submitReply(annotation.id)}
								>
									<Send class="h-3 w-3" />
								</button>
							</div>
						{/if}
					</div>
				{/each}
			</div>
		{/if}
	</div>
</div>
