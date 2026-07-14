<script lang="ts">
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import { BookOpen, Sparkles } from 'lucide-svelte';

	let { bookId } = $props<{ bookId: string }>();

	interface SimilarBook {
		id: string;
		title: string;
		author?: string | null;
		cover_path?: string | null;
		similarity_score?: number;
		reason?: string;
	}

	let similar = $state<SimilarBook[]>([]);
	let loading = $state(true);

	onMount(async () => {
		try {
			similar = await api.getSimilarBooks(bookId);
		} catch {
			// Not available yet
		} finally {
			loading = false;
		}
	});
</script>

<div class="space-y-4">
	<div class="flex items-center gap-2">
		<Sparkles class="w-4 h-4 text-amber-400" />
		<p class="text-sm text-ink-400">基于内容语义和标签的相似书籍推荐</p>
	</div>

	{#if loading}
		<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
			{#each Array(6) as _}
				<div class="rounded-lg border border-ink-800/50 bg-ink-900/30 p-4 animate-pulse">
					<div class="flex gap-3">
						<div class="w-12 h-16 rounded bg-ink-800/50"></div>
						<div class="flex-1 space-y-2">
							<div class="h-3 w-3/4 rounded bg-ink-800/50"></div>
							<div class="h-2 w-1/2 rounded bg-ink-800/30"></div>
						</div>
					</div>
				</div>
			{/each}
		</div>
	{:else if similar.length === 0}
		<div class="py-12 text-center">
			<BookOpen class="w-8 h-8 text-ink-600 mx-auto mb-3" />
			<p class="text-ink-500">暂无相似推荐</p>
			<p class="text-xs text-ink-600 mt-1">AI 分析完成后将自动生成推荐</p>
		</div>
	{:else}
		<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
			{#each similar as book}
				<a
					href="/library/{book.id}"
					class="group rounded-lg border border-ink-800/50 bg-ink-900/30 p-4 hover:border-amber-500/20 transition-colors"
				>
					<div class="flex gap-3">
						<div class="shrink-0 w-12 h-16 rounded bg-gradient-to-br from-ink-800 to-ink-900 overflow-hidden">
							{#if book.cover_path}
								<img src={book.cover_path} alt="" class="w-full h-full object-cover" />
							{/if}
						</div>
						<div class="flex-1 min-w-0">
							<p class="text-sm font-medium text-ink-100 truncate group-hover:text-amber-300 transition-colors">{book.title}</p>
							<p class="text-xs text-ink-500 mt-0.5">{book.author ?? '未知'}</p>
							{#if book.similarity_score}
								<div class="mt-2 flex items-center gap-1.5">
									<div class="h-1 flex-1 rounded-full bg-ink-800 max-w-[60px]">
										<div class="h-full rounded-full bg-amber-500" style="width: {book.similarity_score * 100}%"></div>
									</div>
									<span class="text-[10px] text-ink-500">{Math.round(book.similarity_score * 100)}%</span>
								</div>
							{/if}
							{#if book.reason}
								<p class="text-[11px] text-ink-500 mt-1 line-clamp-2">{book.reason}</p>
							{/if}
						</div>
					</div>
				</a>
			{/each}
		</div>
	{/if}
</div>
