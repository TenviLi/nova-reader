<script lang="ts">
	import * as m from '$lib/paraglide/messages.js';
	import type { DuplicateChapterMatch, DuplicatePair } from '$lib/types/models';

	interface Segment {
		kind: 'shared' | 'unique';
		count: number;
	}

	interface Props {
		pair: DuplicatePair;
		matches?: DuplicateChapterMatch[];
		matchedIndicesA?: number[];
		matchedIndicesB?: number[];
	}

	let { pair, matches = [], matchedIndicesA, matchedIndicesB }: Props = $props();

	function clampRatio(value: number): number {
		return Math.min(1, Math.max(0, value));
	}

	function matchedIndices(side: 'a' | 'b'): Set<number> {
		const explicit = side === 'a' ? matchedIndicesA : matchedIndicesB;
		if (explicit !== undefined) return new Set(explicit);
		return new Set(
			matches
				.map((match) => side === 'a' ? match.chapter_a_index : match.chapter_b_index)
				.filter((index): index is number => index !== null),
		);
	}

	function buildSegments(total: number, coverage: number, indices: Set<number>): Segment[] {
		if (total <= 0) return [];
		if (indices.size === 0) {
			const shared = Math.min(total, Math.round(total * clampRatio(coverage)));
			return [
				...(shared > 0 ? [{ kind: 'shared' as const, count: shared }] : []),
				...(total - shared > 0 ? [{ kind: 'unique' as const, count: total - shared }] : []),
			];
		}

		const segments: Segment[] = [];
		for (let index = 0; index < total; index += 1) {
			const kind = indices.has(index) ? 'shared' : 'unique';
			const previous = segments.at(-1);
			if (previous?.kind === kind) previous.count += 1;
			else segments.push({ kind, count: 1 });
		}
		return segments;
	}

	let totalA = $derived(Math.max(0, pair.book_a.chapter_count));
	let totalB = $derived(Math.max(0, pair.book_b.chapter_count));
	let segmentsA = $derived(buildSegments(totalA, pair.coverage_a, matchedIndices('a')));
	let segmentsB = $derived(buildSegments(totalB, pair.coverage_b, matchedIndices('b')));
	let sharedA = $derived(Math.min(totalA, Math.round(totalA * clampRatio(pair.coverage_a))));
	let sharedB = $derived(Math.min(totalB, Math.round(totalB * clampRatio(pair.coverage_b))));
</script>

<div class="flex flex-col gap-3">
	<div class="flex flex-wrap items-center gap-4 text-xs text-ink-500" aria-hidden="true">
		<span class="flex items-center gap-1.5"><span class="size-2 rounded-sm bg-accent-500"></span>{m.duplicates_track_shared()}</span>
		<span class="flex items-center gap-1.5"><span class="size-2 rounded-sm bg-ink-700"></span>{m.duplicates_track_unique()}</span>
	</div>

	<div class="grid grid-cols-[1.5rem_minmax(0,1fr)_auto] items-center gap-2">
		<span class="font-mono text-xs font-semibold text-ink-300">A</span>
		<div
			class="flex h-3 overflow-hidden rounded-md bg-ink-900 ring-1 ring-ink-800"
			aria-label={m.duplicates_track_aria({ side: 'A', shared: sharedA, unique: Math.max(0, totalA - sharedA), coverage: Math.round(clampRatio(pair.coverage_a) * 100) })}
		>
			{#each segmentsA as segment}
				<span
					class="h-full min-w-px {segment.kind === 'shared' ? 'bg-accent-500' : 'bg-ink-700'}"
					style={`width: ${(segment.count / Math.max(1, totalA)) * 100}%`}
				></span>
			{/each}
		</div>
		<span class="font-mono text-[11px] tabular-nums text-ink-500">{m.duplicates_chapter_count({ count: totalA })}</span>
	</div>

	<div class="grid grid-cols-[1.5rem_minmax(0,1fr)_auto] items-center gap-2">
		<span class="font-mono text-xs font-semibold text-ink-300">B</span>
		<div
			class="flex h-3 overflow-hidden rounded-md bg-ink-900 ring-1 ring-ink-800"
			aria-label={m.duplicates_track_aria({ side: 'B', shared: sharedB, unique: Math.max(0, totalB - sharedB), coverage: Math.round(clampRatio(pair.coverage_b) * 100) })}
		>
			{#each segmentsB as segment}
				<span
					class="h-full min-w-px {segment.kind === 'shared' ? 'bg-accent-500' : 'bg-ink-700'}"
					style={`width: ${(segment.count / Math.max(1, totalB)) * 100}%`}
				></span>
			{/each}
		</div>
		<span class="font-mono text-[11px] tabular-nums text-ink-500">{m.duplicates_chapter_count({ count: totalB })}</span>
	</div>
</div>
