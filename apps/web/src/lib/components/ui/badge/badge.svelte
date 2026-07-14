<script lang="ts">
	import { cn } from '$utils/cn';
	import type { HTMLAttributes } from 'svelte/elements';

	interface Props extends HTMLAttributes<HTMLDivElement> {
		variant?: 'default' | 'outline' | 'secondary';
		class?: string;
	}

	let { variant = 'default', class: className, children, ...restProps }: Props = $props();

	const variants = {
		default: 'border-transparent bg-accent-500/10 text-accent-400',
		secondary: 'border-transparent bg-ink-800 text-ink-200',
		outline: 'border border-ink-700/50 text-ink-300',
	};
</script>

<div
	class={cn(
		'inline-flex items-center rounded-md px-2.5 py-0.5 text-xs font-medium transition-colors',
		variants[variant],
		className,
	)}
	{...restProps}
>
	{@render children?.()}
</div>
