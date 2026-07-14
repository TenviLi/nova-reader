<script lang="ts">
	interface Props {
		value: number;
		max?: number;
		size?: 'xs' | 'sm' | 'md';
		variant?: 'default' | 'accent' | 'success' | 'warning';
		showLabel?: boolean;
		animated?: boolean;
	}

	let {
		value,
		max = 100,
		size = 'sm',
		variant = 'default',
		showLabel = false,
		animated = false,
	}: Props = $props();

	let percent = $derived(Math.min(100, Math.max(0, (value / max) * 100)));

	const heights: Record<string, string> = {
		xs: 'h-1',
		sm: 'h-1.5',
		md: 'h-2.5',
	};

	const colors: Record<string, string> = {
		default: 'bg-ink-400',
		accent: 'bg-accent-500',
		success: 'bg-emerald-500',
		warning: 'bg-amber-500',
	};
</script>

<div class="w-full">
	{#if showLabel}
		<div class="flex justify-between mb-1">
			<span class="text-xs text-ink-400">{Math.round(percent)}%</span>
		</div>
	{/if}
	<div class="w-full rounded-full bg-ink-800/50 {heights[size]} overflow-hidden">
		<div
			class="h-full rounded-full transition-all duration-500 ease-out {colors[variant]}"
			class:animate-pulse={animated && percent < 100}
			style="width: {percent}%"
		></div>
	</div>
</div>
