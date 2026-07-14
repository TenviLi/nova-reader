<script lang="ts">
	import { QueryClient, QueryClientProvider } from '@tanstack/svelte-query';
	import DuplicatePage from '../../routes/library/duplicates/+page.svelte';

	const client = new QueryClient({
		defaultOptions: {
			queries: { retry: false, gcTime: Infinity },
			mutations: { retry: false },
		},
	});
</script>

<QueryClientProvider {client}>
	<DuplicatePage />
</QueryClientProvider>
