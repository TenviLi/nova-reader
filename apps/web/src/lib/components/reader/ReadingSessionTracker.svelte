<script lang="ts">
	/**
	 * ReadingSessionTracker — automatically records reading sessions.
	 * Tracks time on page and reports back on navigation away / visibility change.
	 */
	import { onMount } from 'svelte';
	import { api } from '$services/api';

	let { bookId, chapterIndex = 0, wordCount = 0 } = $props<{
		bookId: string;
		chapterIndex?: number;
		wordCount?: number;
	}>();

	function currentChapterIndex() {
		return chapterIndex;
	}

	let startTime = Date.now();
	let startChapter = currentChapterIndex();
	let totalWordsRead = 0;
	let lastTrackedChapter = currentChapterIndex();

	// Track chapter navigation — only accumulate when chapter actually changes
	$effect(() => {
		if (chapterIndex !== lastTrackedChapter) {
			totalWordsRead += wordCount;
			lastTrackedChapter = chapterIndex;
		}
	});

	function reportSession() {
		const elapsed = Math.floor((Date.now() - startTime) / 1000);
		if (elapsed < 10) return; // Don't report sessions less than 10s
		// Cap session at 30 minutes; auto-segment for analytics
		const cappedDuration = Math.min(elapsed, 1800);

		api.recordReadingSession({
			book_id: bookId,
			start_chapter: startChapter,
			end_chapter: chapterIndex,
			words_read: totalWordsRead,
			duration_secs: cappedDuration,
		}).catch(() => {
			// Silently fail — session tracking is non-critical
		});
	}

	let lastReportTime = 0;

	onMount(() => {
		startTime = Date.now();
		startChapter = chapterIndex;

		// Report on visibility change (tab switch, minimize) — debounced
		function handleVisibility() {
			const now = Date.now();
			if (document.hidden) {
				if (now - lastReportTime > 1000) {
					reportSession();
					lastReportTime = now;
				}
			} else {
				// Resume: start new segment without wiping accumulated words
				startTime = Date.now();
				startChapter = chapterIndex;
			}
		}

		// Report on page unload (navigation away)
		function handleBeforeUnload() {
			reportSession();
		}

		document.addEventListener('visibilitychange', handleVisibility);
		window.addEventListener('beforeunload', handleBeforeUnload);

		return () => {
			reportSession();
			document.removeEventListener('visibilitychange', handleVisibility);
			window.removeEventListener('beforeunload', handleBeforeUnload);
		};
	});
</script>

<!-- This component renders nothing — it's a behavior-only tracker -->
