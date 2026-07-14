<script lang="ts">
	import { cn } from '$lib/utils';

	let {
		value = $bindable(0),
		min = 0,
		max = 100,
		step = 1,
		disabled = false,
		class: className = '',
		onchange,
		...restProps
	} = $props<{
		value?: number;
		min?: number;
		max?: number;
		step?: number;
		disabled?: boolean;
		class?: string;
		onchange?: (value: number) => void;
	}>();

	let percentage = $derived(((value - min) / (max - min)) * 100);
</script>

<div class={cn('relative flex w-full touch-none select-none items-center', className)} {...restProps}>
	<div class="relative h-1.5 w-full grow overflow-hidden rounded-full bg-ink-800">
		<div
			class="absolute h-full bg-accent-500 rounded-full transition-all duration-75"
			style="width: {percentage}%"
		></div>
	</div>
	<input
		type="range"
		bind:value
		{min}
		{max}
		{step}
		{disabled}
		oninput={() => onchange?.(value)}
		class="absolute inset-0 h-full w-full cursor-pointer opacity-0"
	/>
	<div
		class="absolute h-4 w-4 rounded-full border-2 border-accent-500 bg-ink-950 shadow-sm ring-offset-ink-950 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-500 focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 pointer-events-none"
		style="left: calc({percentage}% - 8px)"
	></div>
</div>
