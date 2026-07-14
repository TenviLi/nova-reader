<script lang="ts">
	import { Users, MapPin, Building2, Swords, Zap, Calendar, Lightbulb } from 'lucide-svelte';
	import type { ComponentType } from 'svelte';

	let { entity } = $props<{
		entity: {
			id: string;
			name: string;
			type: string;
			aliases?: string[];
			description?: string;
			mention_count: number;
			first_book_id?: string;
			first_book_title?: string;
			top_relations?: Array<{ type: string; target: string }>;
		};
	}>();

	const typeConfig: Record<string, { icon: ComponentType; bg: string; text: string }> = {
		character: { icon: Users, bg: 'bg-amber-500/10', text: 'text-amber-400' },
		location: { icon: MapPin, bg: 'bg-emerald-500/10', text: 'text-emerald-400' },
		organization: { icon: Building2, bg: 'bg-indigo-500/10', text: 'text-indigo-400' },
		item: { icon: Swords, bg: 'bg-pink-500/10', text: 'text-pink-400' },
		skill: { icon: Zap, bg: 'bg-rose-500/10', text: 'text-rose-400' },
		event: { icon: Calendar, bg: 'bg-cyan-500/10', text: 'text-cyan-400' },
		concept: { icon: Lightbulb, bg: 'bg-violet-500/10', text: 'text-violet-400' },
	};

	const config = $derived(typeConfig[entity.type] ?? typeConfig.concept);
	let Icon = $derived(config.icon);
</script>

<div class="w-80 rounded-xl border border-ink-700/50 bg-ink-900 p-4 shadow-2xl">
	<div class="flex items-start gap-3">
		<div class="rounded-lg p-2 {config.bg}">
			<Icon size={18} strokeWidth={1.8} class={config.text} />
		</div>
		<div class="min-w-0 flex-1">
			<h4 class="truncate font-semibold text-ink-100">{entity.name}</h4>
			{#if entity.aliases?.length}
				<p class="truncate text-[11px] text-ink-500">{entity.aliases.join(' / ')}</p>
			{/if}
		</div>
		<div class="text-right">
			<div class="text-lg font-bold tabular-nums text-accent-400">{entity.mention_count}</div>
			<div class="text-[10px] text-ink-500">提及</div>
		</div>
	</div>

	{#if entity.description}
		<p class="mt-3 line-clamp-3 text-xs leading-relaxed text-ink-400">{entity.description}</p>
	{/if}

	{#if entity.top_relations?.length}
		<div class="mt-3 flex flex-wrap gap-1">
			{#each entity.top_relations.slice(0, 3) as rel}
				<span class="rounded bg-ink-800/80 px-1.5 py-0.5 text-[10px] text-ink-300">
					{rel.type} → {rel.target}
				</span>
			{/each}
		</div>
	{/if}

	<div class="mt-3 flex items-center gap-1.5 border-t border-ink-800/50 pt-3">
		<a href="/characters/{entity.id}" class="text-[11px] text-accent-400 hover:underline">
			查看详情
		</a>
		{#if entity.first_book_id}
			<span class="text-ink-600">·</span>
			<a href="/library/{entity.first_book_id}" class="text-[11px] text-ink-400 hover:text-ink-200">
				首次出现: {entity.first_book_title}
			</a>
		{/if}
	</div>
</div>
