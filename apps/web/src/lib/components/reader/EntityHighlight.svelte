<script lang="ts">
	import type { Snippet } from 'svelte';

	let { name, type, id, children } = $props<{
		name: string;
		type: string;
		id: string;
		children?: Snippet;
	}>();
</script>

<span
	class="entity-highlight entity-{type} cursor-pointer border-b-2 border-dotted transition-colors hover:bg-accent-500/10"
	data-entity-id={id}
	title={name}
>
	{@render children?.()}
</span>
