<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import { api } from '$services/api';
	import { BookOpen, Clock, ChevronRight } from 'lucide-svelte';
	import BookCard from '$components/library/BookCard.svelte';

	const reading = createQuery(() => ({
		queryKey: ['books', 'currently-reading'],
		queryFn: () => api.getBooks({ reading_status: 'reading', sort_by: 'updated_at', per_page: 6 }),
		staleTime: 30_000,
		retry: false,
	}));

	const recent = createQuery(() => ({
		queryKey: ['books', 'recent-reading-fallback'],
		queryFn: () => api.getBooks({ sort_by: 'updated_at', per_page: 6 }),
		staleTime: 30_000,
		retry: false,
	}));

	let mergedBooks = $derived.by(() => {
		const byId = new Map();
		for (const book of reading.data?.data ?? []) byId.set(book.id, book);
		for (const book of recent.data?.data ?? []) {
			if (byId.size >= 6) break;
			if (!byId.has(book.id)) byId.set(book.id, book);
		}
		return [...byId.values()];
	});

</script>

{#if mergedBooks.length > 0}
	<div class="rounded-xl border border-ink-800/50 bg-ink-900/80 p-5">
		<div class="flex items-center justify-between mb-4">
			<div>
				<h2 class="text-lg font-semibold text-ink-100 flex items-center gap-2">
					<BookOpen class="w-5 h-5 text-amber-400" />
					继续阅读
				</h2>
				<p class="mt-0.5 flex items-center gap-1 text-xs text-ink-500">
					<Clock class="h-3 w-3" />
					在读书籍优先，最近打开自动补位
				</p>
			</div>
			<a href="/reading" class="text-xs text-ink-500 hover:text-ink-300 transition-colors flex items-center gap-0.5">
				查看全部 <ChevronRight class="w-3 h-3" />
			</a>
		</div>

		<div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
			{#each mergedBooks as book}
				<BookCard
					{book}
					variant="compact"
					href="/library/{book.id}"
					readHref="/reading/{book.id}"
					eyebrow={book.reading_status === 'reading' ? '正在阅读' : '最近阅读'}
					showBadge={false}
					showFormat={false}
				/>
			{/each}
		</div>
	</div>
{/if}
