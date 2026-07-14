<script lang="ts">
	import type { ComponentType } from 'svelte';
	import { TrendingUp, TrendingDown } from 'lucide-svelte';

	type Variant = 'default' | 'active';

	let { label, value, icon, trend, variant = 'default' } = $props<{
		label: string;
		value: string | number;
		icon: ComponentType;
		trend?: number;
		variant?: Variant;
	}>();

	let Icon = $derived(icon);
</script>

<div
	class="group relative overflow-hidden rounded-xl border border-ink-800/40 bg-ink-900/40 p-4 transition-all duration-200 hover:border-ink-700/50 hover:bg-ink-900/60 {variant === 'active' ? 'border-accent-500/20' : ''} {variant === 'active' ? 'bg-accent-500/5' : ''}"
>
	<!-- Icon -->
	<div
		class="mb-3 flex h-9 w-9 items-center justify-center rounded-lg {variant === 'default' ? 'bg-ink-800/80' : ''} {variant === 'active' ? 'bg-accent-500/10' : ''}"
	>
		<Icon
			size={16}
			strokeWidth={1.8}
			class={variant === 'active' ? 'text-accent-400' : 'text-ink-400'}
		/>
	</div>

	<!-- Value -->
	<div class="text-2xl font-bold text-ink-50 tabular-nums">{value}</div>

	<!-- Label & Trend -->
	<div class="mt-1 flex items-center justify-between">
		<span class="text-xs text-ink-500">{label}</span>
		{#if trend}
			<span
				class="inline-flex items-center gap-0.5 text-xs font-medium"
				class:text-success={trend > 0}
				class:text-error={trend < 0}
			>
				{#if trend > 0}
					<TrendingUp size={10} />
				{:else}
					<TrendingDown size={10} />
				{/if}
				{trend > 0 ? '+' : ''}{trend}%
			</span>
		{/if}
	</div>

	<!-- Subtle glow for active cards -->
	{#if variant === 'active'}
		<div class="absolute -right-4 -top-4 h-16 w-16 rounded-full bg-accent-500/10 blur-2xl"></div>
	{/if}
</div>
