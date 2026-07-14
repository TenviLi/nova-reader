<script lang="ts">
	import EyeIcon from '@lucide/svelte/icons/eye';

	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import * as Card from '$lib/components/ui/card';
	import * as m from '$lib/paraglide/messages.js';
	import { getLocale } from '$lib/paraglide/runtime.js';
	import type { DuplicatePair, DuplicateRelation } from '$lib/types/models';
	import ChapterOverlapTrack from './ChapterOverlapTrack.svelte';
	import SemanticEvidencePanel from './SemanticEvidencePanel.svelte';

	interface Props {
		pair: DuplicatePair;
		onview: (id: string) => void;
	}

	let { pair, onview }: Props = $props();

	const relationLabels: Record<DuplicateRelation, string> = {
		exact_file: m.duplicates_relation_exact_file(),
		exact_content: m.duplicates_relation_exact_content(),
		contained_version: m.duplicates_relation_contained_version(),
		high_overlap: m.duplicates_relation_high_overlap(),
		partial_overlap: m.duplicates_relation_partial_overlap(),
		semantic_relation: m.duplicates_relation_semantic_review(),
	};

	function formatWords(count: number): string {
		const formatted = new Intl.NumberFormat(getLocale(), {
			notation: count >= 10_000 ? 'compact' : 'standard',
			maximumFractionDigits: 1,
		}).format(count);
		return m.duplicates_word_count({ count: formatted });
	}

	function authorLabel(author: string | null): string {
		return author?.trim() || m.duplicates_unknown_author();
	}
</script>

<Card.Root>
	<Card.Header class="gap-3">
		<div class="flex flex-wrap items-center gap-2">
			<Badge>{relationLabels[pair.relation]}</Badge>
			<Badge variant="outline">{m.duplicates_confidence({ score: Math.round(pair.confidence * 100) })}</Badge>
			{#if pair.relation !== 'semantic_relation'}
				<span class="text-xs font-medium text-ink-300">{m.duplicates_shared_count({ count: pair.shared_chapters })}</span>
				<span class="text-xs text-ink-500">{m.duplicates_longest_shared_count({ count: pair.longest_contiguous_run })}</span>
			{/if}
		</div>
		<Card.Title class="sr-only">{m.duplicates_pair_accessible_title({ bookA: pair.book_a.title, bookB: pair.book_b.title })}</Card.Title>
		<Card.Description>
			{pair.relation === 'contained_version'
				? m.duplicates_contained_hint()
				: pair.relation === 'semantic_relation'
					? m.duplicates_semantic_hint()
					: m.duplicates_overlap_hint()}
		</Card.Description>
	</Card.Header>

	<Card.Content class="flex flex-col gap-5">
		<div class="grid gap-3 lg:grid-cols-2">
			{#each [['A', pair.book_a, pair.coverage_a, pair.character_coverage_a], ['B', pair.book_b, pair.coverage_b, pair.character_coverage_b]] as item}
				{@const [side, book, chapterCoverage, characterCoverage] = item as ['A' | 'B', typeof pair.book_a, number, number]}
				<div class="flex min-w-0 items-start gap-3 rounded-lg bg-ink-950/45 p-3 ring-1 ring-ink-800/70">
					<div class="flex size-8 shrink-0 items-center justify-center rounded-md bg-ink-800 font-mono text-xs font-semibold text-ink-300">{side}</div>
					<div class="min-w-0 flex-1">
						<div class="flex flex-wrap items-center gap-2">
							<a href={`/library/${book.id}`} class="truncate text-sm font-semibold text-ink-100 hover:text-accent-400">{book.title}</a>
							{#if pair.recommended_primary_id === book.id}<Badge variant="secondary">{m.duplicates_recommended_primary()}</Badge>{/if}
						</div>
						<p class="mt-1 text-xs text-ink-500">{authorLabel(book.author)} · {book.format.toUpperCase()} · {m.duplicates_chapter_count({ count: book.chapter_count })} · {formatWords(book.word_count)}</p>
						{#if pair.relation !== 'semantic_relation'}
							<p class="mt-2 text-xs text-ink-400">{m.duplicates_chapter_coverage({ chapter: Math.round(chapterCoverage * 100), character: Math.round(characterCoverage * 100) })}</p>
						{/if}
					</div>
				</div>
			{/each}
		</div>

		{#if pair.relation === 'semantic_relation'}
			<SemanticEvidencePanel {pair} />
		{:else}
			<ChapterOverlapTrack {pair} />
		{/if}
	</Card.Content>

	<Card.Footer class="justify-end">
		<Button variant="outline" onclick={() => onview(pair.id)}>
			<EyeIcon data-icon="inline-start" />
			{pair.relation === 'semantic_relation' ? m.duplicates_view_semantic_evidence() : m.duplicates_view_evidence()}
		</Button>
	</Card.Footer>
</Card.Root>
