<script lang="ts">
	let { text, highlights = [], query = '' } = $props<{
		text: string;
		highlights?: Array<{ start: number; end: number; type?: string }>;
		query?: string;
	}>();

	interface Segment {
		text: string;
		highlighted: boolean;
		type?: string;
	}

	let segments = $derived.by(() => {
		if (highlights.length === 0 && !query) {
			return [{ text, highlighted: false }] as Segment[];
		}

		// If we have explicit highlight ranges, use them
		if (highlights.length > 0) {
			const sorted = [...highlights].sort((a, b) => a.start - b.start);
			const result: Segment[] = [];
			let lastEnd = 0;

			for (const h of sorted) {
				if (h.start > lastEnd) {
					result.push({ text: text.slice(lastEnd, h.start), highlighted: false });
				}
				result.push({ text: text.slice(h.start, h.end), highlighted: true, type: h.type });
				lastEnd = h.end;
			}
			if (lastEnd < text.length) {
				result.push({ text: text.slice(lastEnd), highlighted: false });
			}
			return result;
		}

		// Otherwise highlight query string matches (case-insensitive)
		if (query) {
			// Split query into terms (space-separated) and highlight each
			const terms = query.trim().split(/\s+/).filter((t: string) => t.length > 0);
			const escapedTerms = terms.map((t: string) => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
			const pattern = escapedTerms.join('|');
			if (!pattern) return [{ text, highlighted: false }] as Segment[];
			const regex = new RegExp(`(${pattern})`, 'gi');
			const parts: string[] = text.split(regex);
			return parts.filter((p: string) => p.length > 0).map((part: string) => ({
				text: part,
				highlighted: new RegExp(`^(${pattern})$`, 'i').test(part),
			})) as Segment[];
		}

		return [{ text, highlighted: false }] as Segment[];
	});

	const typeColors: Record<string, string> = {
		person: 'bg-entity-person/20 text-entity-person',
		location: 'bg-entity-location/20 text-entity-location',
		organization: 'bg-entity-organization/20 text-entity-organization',
		item: 'bg-entity-item/20 text-entity-item',
		event: 'bg-entity-event/20 text-entity-event',
		match: 'bg-accent-500/20 text-accent-200',
	};
</script>

<span class="highlighted-text">
	{#each segments as segment}
		{#if segment.highlighted}
			<mark class="rounded px-0.5 {typeColors[segment.type ?? 'match'] ?? 'bg-accent-500/20 text-accent-200'}">{segment.text}</mark>
		{:else}
			{segment.text}
		{/if}
	{/each}
</span>
