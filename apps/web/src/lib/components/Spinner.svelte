<script lang="ts">
	let { size = 'md', label = '' } = $props<{ size?: 'sm' | 'md' | 'lg'; label?: string }>();

	const sizes: Record<string, string> = {
		sm: 'w-5 h-5 border-2',
		md: 'w-8 h-8 border-2',
		lg: 'w-12 h-12 border-3',
	};
</script>

<div class="flex flex-col items-center justify-center gap-3">
	<div class="animate-spin rounded-full border-accent-500 border-t-transparent {sizes[size]}"></div>
	{#if label}
		<p class="text-sm text-ink-400 animate-pulse">{label}</p>
	{/if}
</div>
