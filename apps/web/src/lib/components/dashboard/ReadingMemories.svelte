<script lang="ts">
	import { api } from '$services/api';
	import { BookOpen, Calendar } from 'lucide-svelte';
	import { onMount } from 'svelte';

	let memories = $state.raw<Array<{
		book_id: string;
		title: string;
		author: string | null;
		cover_path: string | null;
		read_date: string;
		years_ago: number;
	}>>([]);
	let loading = $state(true);

	onMount(async () => {
		try {
			// Find reading sessions from the same date in previous years
			const today = new Date();
			const month = today.getMonth() + 1;
			const day = today.getDate();
			const result = await api.getReadingMemories(month, day);
			if (result && result.length > 0) {
				memories = result;
			}
		} catch {
			// Graceful fallback — this endpoint may not exist yet
			// Use reading sessions as fallback
			try {
				const sessions = await api.getReadingSessions({ limit: 100 });
				if (sessions) {
					const today = new Date();
					const todayMD = `${today.getMonth() + 1}-${today.getDate()}`;
					const pastSessions = sessions.filter((s) => {
						if (!s.started_at) return false;
						const d = new Date(s.started_at);
						const sMD = `${d.getMonth() + 1}-${d.getDate()}`;
						return sMD === todayMD && d.getFullYear() < today.getFullYear();
					});
					const bookIds = [...new Set(pastSessions.map((s) => s.book_id))];
					memories = bookIds.slice(0, 3).map((bid: string) => {
						const session = pastSessions.find((s) => s.book_id === bid)!;
						const sessionDate = new Date(session.started_at);
						return {
							book_id: bid,
							title: session.book_title ?? '未知书名',
							author: null,
							cover_path: null,
							read_date: session.started_at,
							years_ago: today.getFullYear() - sessionDate.getFullYear(),
						};
					});
				}
			} catch { /* no-op */ }
		} finally {
			loading = false;
		}
	});
</script>

{#if !loading && memories.length > 0}
	<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
		<div class="flex items-center gap-2 mb-3">
			<Calendar size={16} strokeWidth={1.5} class="text-amber-400" />
			<h3 class="text-sm font-semibold text-ink-100">阅读回忆</h3>
		</div>
		<div class="space-y-2.5">
			{#each memories as memory}
				<a
					href="/library/{memory.book_id}"
					class="group flex items-center gap-3 rounded-lg p-2 -mx-2 hover:bg-ink-800/40 transition-colors"
				>
					<div class="h-10 w-7 shrink-0 overflow-hidden rounded bg-ink-800 shadow-sm">
						{#if memory.cover_path}
							<img src="/api/covers/{memory.book_id}" alt="" class="h-full w-full object-cover" />
						{:else}
							<div class="flex h-full items-center justify-center text-[8px] text-ink-500">
								<BookOpen size={12} />
							</div>
						{/if}
					</div>
					<div class="flex-1 min-w-0">
						<p class="text-sm text-ink-200 truncate group-hover:text-accent-400 transition-colors">{memory.title}</p>
						<p class="text-[10px] text-ink-500">
							{memory.years_ago} 年前的今天在读
							{#if memory.author}· {memory.author}{/if}
						</p>
					</div>
				</a>
			{/each}
		</div>
	</div>
{/if}
