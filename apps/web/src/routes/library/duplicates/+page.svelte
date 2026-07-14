<script lang="ts">
	import AlertTriangleIcon from '@lucide/svelte/icons/triangle-alert';
	import CheckCircleIcon from '@lucide/svelte/icons/circle-check';
	import LayersIcon from '@lucide/svelte/icons/layers-3';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import ScanSearchIcon from '@lucide/svelte/icons/scan-search';
	import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
	import { createMutation, createQuery, useQueryClient } from '@tanstack/svelte-query';
	import { toast } from 'svelte-sonner';

	import ChapterOverlapTrack from '$lib/components/duplicates/ChapterOverlapTrack.svelte';
	import ChapterMatchDiff from '$lib/components/duplicates/ChapterMatchDiff.svelte';
	import DuplicatePairCard from '$lib/components/duplicates/DuplicatePairCard.svelte';
	import GroupedAlignmentPanel from '$lib/components/duplicates/GroupedAlignmentPanel.svelte';
	import PrimaryRecommendationPanel from '$lib/components/duplicates/PrimaryRecommendationPanel.svelte';
	import SemanticEvidencePanel from '$lib/components/duplicates/SemanticEvidencePanel.svelte';
	import * as Alert from '$lib/components/ui/alert';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import * as Card from '$lib/components/ui/card';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import * as Dialog from '$lib/components/ui/dialog';
	import ProgressBar from '$lib/components/ui/ProgressBar.svelte';
	import * as Select from '$lib/components/ui/select';
	import { Separator } from '$lib/components/ui/separator';
	import * as Sheet from '$lib/components/ui/sheet';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import * as Tabs from '$lib/components/ui/tabs';
	import * as m from '$lib/paraglide/messages.js';
	import { api } from '$services/api';
	import type {
		DuplicateBookSummary,
		DuplicateCandidateKind,
		DuplicatePairStatus,
		DuplicateRelation,
		DuplicateResolutionAction,
		DuplicateScanPhase,
	} from '$lib/types/models';
	import { formatLocaleNumber as formatNumber, getErrorMessage } from '$lib/utils';

	const queryClient = useQueryClient();
	const PAIR_PAGE_SIZE = 20;
	const MATCH_PAGE_SIZE = 50;
	const DISCOVERY_LIMIT = 5;
	type ContentDuplicateRelation = Exclude<DuplicateRelation, 'semantic_relation'>;
	const scanPhaseLabels: Record<DuplicateScanPhase, () => string> = {
		recovering: m.duplicates_scan_phase_recovering,
		retrying: m.duplicates_scan_phase_retrying,
		failed: m.duplicates_scan_phase_failed,
		fingerprinting: m.duplicates_scan_phase_fingerprinting,
		candidate_generation: m.duplicates_scan_phase_candidate_generation,
		verifying: m.duplicates_scan_phase_verifying,
		completed: m.duplicates_scan_phase_completed,
	};

	let selectedLibraryId = $state('all');
	let candidateKind = $state<DuplicateCandidateKind>('content');
	let relationFilter = $state<'all' | ContentDuplicateRelation>('all');
	let statusFilter = $state<'all' | DuplicatePairStatus>('pending');
	let includeSemantic = $state(false);
	let selectedPairId = $state<string | null>(null);
	let detailOpen = $state(false);
	let confirmOpen = $state(false);
	let confirmAction = $state<'keep_a' | 'keep_b' | null>(null);
	let refreshedScanId = $state<string | null>(null);
	let pairPage = $state(0);
	let matchPage = $state(0);
	let lastFilterKey = '';

	const relationOptions: Array<{ value: 'all' | ContentDuplicateRelation; label: string }> = [
		{ value: 'all', label: m.duplicates_relation_all() },
		{ value: 'exact_file', label: m.duplicates_relation_exact_file() },
		{ value: 'exact_content', label: m.duplicates_relation_exact_content() },
		{ value: 'contained_version', label: m.duplicates_relation_contained_version() },
		{ value: 'high_overlap', label: m.duplicates_relation_high_overlap() },
		{ value: 'partial_overlap', label: m.duplicates_relation_partial_overlap() },
	];

	const statusOptions: Array<{ value: 'all' | DuplicatePairStatus; label: string }> = [
		{ value: 'pending', label: m.duplicates_status_pending() },
		{ value: 'deferred', label: m.duplicates_status_deferred() },
		{ value: 'confirmed', label: m.duplicates_status_confirmed() },
		{ value: 'dismissed', label: m.duplicates_status_dismissed() },
		{ value: 'all', label: m.duplicates_status_all() },
	];
	let pairRelation = $derived(
		candidateKind === 'content' && relationFilter !== 'all' ? relationFilter : undefined,
	);

	const libraries = createQuery(() => ({
		queryKey: ['libraries'],
		queryFn: () => api.getLibraries(),
		staleTime: 30_000,
	}));

	const latestScan = createQuery(() => ({
		queryKey: ['duplicates', 'scan', 'latest', selectedLibraryId],
		queryFn: () => api.getLatestDuplicateScan(selectedLibraryId === 'all' ? undefined : selectedLibraryId),
		refetchInterval: (query) => {
			const status = query.state.data?.status;
			return status === 'queued' || status === 'running' ? 1_500 : false;
		},
	}));

	const pairs = createQuery(() => ({
		queryKey: ['duplicates', 'pairs', candidateKind, pairRelation, statusFilter, selectedLibraryId, pairPage],
		queryFn: () => api.getDuplicatePairs({
			candidate_kind: candidateKind,
			relation: pairRelation,
			status: statusFilter === 'all' ? undefined : statusFilter,
			library_id: selectedLibraryId === 'all' ? undefined : selectedLibraryId,
			limit: PAIR_PAGE_SIZE,
			offset: pairPage * PAIR_PAGE_SIZE,
		}),
	}));

	const exactDiscoveries = createQuery(() => ({
		queryKey: ['duplicates', 'exact-file-discoveries', selectedLibraryId],
		queryFn: () => api.getExactFileDiscoveries({
			library_id: selectedLibraryId === 'all' ? undefined : selectedLibraryId,
			limit: DISCOVERY_LIMIT,
			offset: 0,
		}),
	}));

	const pairDetail = createQuery(() => ({
		queryKey: ['duplicates', 'pair', selectedPairId, matchPage],
		queryFn: () => api.getDuplicatePair(selectedPairId!, {
			match_limit: MATCH_PAGE_SIZE,
			match_offset: matchPage * MATCH_PAGE_SIZE,
		}),
		enabled: detailOpen && selectedPairId !== null,
	}));

	const startScan = createMutation(() => ({
		mutationFn: () => api.startDuplicateScan({
			library_id: selectedLibraryId === 'all' ? undefined : selectedLibraryId,
			include_semantic: includeSemantic,
		}),
		onSuccess: (scan) => {
			queryClient.setQueryData(['duplicates', 'scan', 'latest', selectedLibraryId], scan);
			toast.success(m.duplicates_scan_started());
		},
		onError: (error) => toast.error(m.duplicates_scan_start_failed({ error: getErrorMessage(error) })),
	}));

	const resolvePair = createMutation(() => ({
		mutationFn: ({ pairId, action }: { pairId: string; action: DuplicateResolutionAction }) =>
			api.resolveDuplicatePair(pairId, { action }),
		onSuccess: (_result, variables) => {
			const messages: Record<DuplicateResolutionAction, string> = {
				keep_a: m.duplicates_resolution_keep_a(),
				keep_b: m.duplicates_resolution_keep_b(),
				same_work: m.duplicates_resolution_same_work(),
				dismiss: m.duplicates_resolution_dismiss(),
				defer: m.duplicates_resolution_defer(),
			};
			toast.success(messages[variables.action]);
			confirmOpen = false;
			detailOpen = false;
			pairPage = 0;
			void queryClient.invalidateQueries({ queryKey: ['duplicates', 'pairs'] });
		},
		onError: (error) => toast.error(m.duplicates_resolution_failed({ error: getErrorMessage(error) })),
	}));

	let pairItems = $derived(
		(pairs.data?.items ?? []).filter((pair) =>
			candidateKind === 'semantic'
				? pair.relation === 'semantic_relation'
				: pair.relation !== 'semantic_relation',
		),
	);
	let selectedPair = $derived(pairItems.find((pair) => pair.id === selectedPairId) ?? pairDetail.data ?? null);
	let scan = $derived(latestScan.data ?? null);
	let scanRunning = $derived(scan?.status === 'queued' || scan?.status === 'running');
	let confirmPrimary = $derived(
		confirmAction === 'keep_a' ? selectedPair?.book_a ?? null : selectedPair?.book_b ?? null,
	);
	let confirmSecondary = $derived(
		confirmAction === 'keep_a' ? selectedPair?.book_b ?? null : selectedPair?.book_a ?? null,
	);
	let pairRangeStart = $derived((pairs.data?.total ?? 0) === 0 ? 0 : (pairs.data?.offset ?? 0) + 1);
	let pairRangeEnd = $derived(Math.min((pairs.data?.offset ?? 0) + pairItems.length, pairs.data?.total ?? 0));
	let matchItems = $derived(pairDetail.data?.chapter_matches ?? []);
	let matchTotal = $derived(pairDetail.data?.chapter_matches_total ?? 0);
	let matchRangeStart = $derived(matchTotal === 0 ? 0 : (pairDetail.data?.chapter_matches_offset ?? 0) + 1);
	let matchRangeEnd = $derived(Math.min((pairDetail.data?.chapter_matches_offset ?? 0) + matchItems.length, matchTotal));

	$effect(() => {
		if (scan?.status === 'completed' && refreshedScanId !== scan.id) {
			refreshedScanId = scan.id;
			void queryClient.invalidateQueries({ queryKey: ['duplicates', 'pairs'] });
		}
	});

	$effect(() => {
		const filterKey = `${candidateKind}:${selectedLibraryId}:${relationFilter}:${statusFilter}`;
		if (lastFilterKey && filterKey !== lastFilterKey) pairPage = 0;
		lastFilterKey = filterKey;
	});

	function openPair(id: string): void {
		selectedPairId = id;
		matchPage = 0;
		detailOpen = true;
	}

	function changeCandidateKind(value: string): void {
		if (value !== 'content' && value !== 'semantic') return;
		pairPage = 0;
		candidateKind = value;
	}

	function submitResolution(action: DuplicateResolutionAction): void {
		if (!selectedPairId) return;
		resolvePair.mutate({ pairId: selectedPairId, action });
	}

	function requestKeep(action: 'keep_a' | 'keep_b'): void {
		confirmAction = action;
		confirmOpen = true;
	}

	function scanPhaseLabel(phase: DuplicateScanPhase | null): string {
		return phase === null ? m.duplicates_scan_status_updated() : scanPhaseLabels[phase]();
	}

	function chapterLabel(index: number | null, title: string | null): string {
		if (index === null) return '—';
		return title
			? m.duplicates_chapter_label_with_title({ number: index + 1, title })
			: m.duplicates_chapter_label({ number: index + 1 });
	}

	function bookLabel(book: DuplicateBookSummary | null): string {
		return book ? m.duplicates_book_label({ title: book.title }) : m.duplicates_this_version();
	}
</script>

<svelte:head>
	<title>{m.duplicates_page_title()}</title>
</svelte:head>

<div class="mx-auto flex max-w-[1500px] flex-col gap-6 px-4 py-6 sm:px-6 lg:px-8">
	<header class="flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
		<div class="max-w-3xl">
			<div class="mb-2 flex items-center gap-2 text-xs font-medium uppercase tracking-[0.18em] text-ink-500">
				<LayersIcon class="size-4" />
				{m.duplicates_eyebrow()}
			</div>
			<h1 class="text-2xl font-bold tracking-tight text-ink-50 sm:text-3xl">{m.duplicates_title()}</h1>
			<p class="mt-2 text-sm leading-6 text-ink-400">{m.duplicates_subtitle()}</p>
		</div>
		<Button variant="outline" onclick={() => {
			void latestScan.refetch();
			void pairs.refetch();
			void exactDiscoveries.refetch();
		}}>
			<RefreshCwIcon data-icon="inline-start" />
			{m.duplicates_refresh_results()}
		</Button>
	</header>

	<Card.Root>
		<Card.Header>
			<Card.Title>{m.duplicates_scan_title()}</Card.Title>
			<Card.Description>{m.duplicates_scan_description()}</Card.Description>
			<Card.Action>
				<Button onclick={() => startScan.mutate()} disabled={scanRunning || startScan.isPending}>
					<ScanSearchIcon data-icon="inline-start" />
					{scanRunning ? m.duplicates_scan_running() : selectedLibraryId === 'all' ? m.duplicates_scan_all() : m.duplicates_scan_selected()}
				</Button>
			</Card.Action>
		</Card.Header>
		<Card.Content class="flex flex-col gap-5">
			<div class="flex flex-col gap-3 sm:flex-row sm:items-center">
				<label for="scan-library" class="text-sm font-medium text-ink-300">{m.duplicates_scan_scope()}</label>
				<Select.Root type="single" bind:value={selectedLibraryId}>
					<Select.Trigger id="scan-library" aria-label={m.duplicates_scan_library_aria()} class="w-full sm:w-64">
						{selectedLibraryId === 'all' ? m.duplicates_all_libraries() : libraries.data?.find((library) => library.id === selectedLibraryId)?.name ?? m.duplicates_selected_library()}
					</Select.Trigger>
					<Select.Content>
						<Select.Group>
							<Select.Item value="all">{m.duplicates_all_libraries()}</Select.Item>
							{#each libraries.data ?? [] as library}
								<Select.Item value={library.id}>{library.name}</Select.Item>
							{/each}
						</Select.Group>
					</Select.Content>
				</Select.Root>
				<label class="flex cursor-pointer items-center gap-2 text-sm text-ink-400">
					<Checkbox bind:checked={includeSemantic} aria-label={m.duplicates_include_semantic()} />
					{m.duplicates_include_semantic()}
				</label>
			</div>

			<Separator />

			{#if latestScan.isLoading}
				<div class="flex flex-col gap-3"><Skeleton class="h-4 w-40" /><Skeleton class="h-2 w-full" /></div>
			{:else if latestScan.isError}
				<div class="flex items-center gap-2 text-sm text-destructive"><AlertTriangleIcon class="size-4" />{m.duplicates_scan_status_failed({ error: getErrorMessage(latestScan.error) })}</div>
			{:else if scan}
				<div class="flex flex-col gap-4">
					<div class="flex flex-wrap items-center justify-between gap-2">
						<div class="flex items-center gap-2">
							<Badge variant={scan.status === 'failed' ? 'outline' : 'secondary'}>
								{scan.status === 'queued' ? m.duplicates_scan_status_queued() : scan.status === 'running' ? m.duplicates_scan_status_running() : scan.status === 'completed' ? m.duplicates_scan_status_completed() : m.duplicates_scan_status_failed_label()}
							</Badge>
							<span class="text-sm text-ink-300">{scanPhaseLabel(scan.progress_message)}</span>
						</div>
						<span class="font-mono text-xs tabular-nums text-ink-500">{scan.progress}%</span>
					</div>
					<ProgressBar value={scan.progress} animated={scanRunning} variant={scan.status === 'failed' ? 'warning' : 'accent'} />
					<div class="grid grid-cols-2 gap-3 sm:grid-cols-4 lg:grid-cols-7">
						{#each [
							[m.duplicates_metric_books_processed(), `${formatNumber(scan.books_processed)} / ${formatNumber(scan.books_total)}`],
							[m.duplicates_metric_chapters_processed(), formatNumber(scan.chapters_processed)],
							[m.duplicates_metric_candidates(), formatNumber(scan.candidates_found)],
							[m.duplicates_metric_pairs(), formatNumber(scan.pairs_found)],
							[m.duplicates_metric_exact(), formatNumber(scan.exact_pairs)],
							[m.duplicates_metric_contained(), formatNumber(scan.contained_pairs)],
							[m.duplicates_metric_semantic(), formatNumber(scan.semantic_pairs)],
						] as metric}
							<div class="rounded-lg bg-ink-950/45 p-3 ring-1 ring-ink-800/60">
								<p class="text-[11px] text-ink-500">{metric[0]}</p>
								<p class="mt-1 font-mono text-sm font-semibold tabular-nums text-ink-200">{metric[1]}</p>
							</div>
						{/each}
					</div>
					{#if scan.error_message}<p class="text-sm text-destructive">{scan.error_message}</p>{/if}
				</div>
			{:else}
				<p class="text-sm text-ink-500">{m.duplicates_scan_empty()}</p>
			{/if}
		</Card.Content>
	</Card.Root>

	<Card.Root>
		<Card.Header>
			<Card.Title>{m.duplicates_exact_discoveries_title()}</Card.Title>
			<Card.Description>{m.duplicates_exact_discoveries_description()}</Card.Description>
			{#if (exactDiscoveries.data?.total ?? 0) > 0}
				<Card.Action><Badge variant="secondary">{formatNumber(exactDiscoveries.data?.total ?? 0)}</Badge></Card.Action>
			{/if}
		</Card.Header>
		<Card.Content>
			{#if exactDiscoveries.isLoading}
				<div class="flex flex-col gap-2"><Skeleton class="h-12 w-full" /><Skeleton class="h-12 w-full" /></div>
			{:else if exactDiscoveries.isError}
				<p class="text-sm text-destructive">{m.duplicates_exact_discoveries_load_failed({ error: getErrorMessage(exactDiscoveries.error) })}</p>
			{:else if (exactDiscoveries.data?.items.length ?? 0) === 0}
				<p class="text-sm text-ink-500">{m.duplicates_exact_discoveries_empty()}</p>
			{:else}
				<ul class="divide-y divide-ink-800/70">
					{#each exactDiscoveries.data?.items ?? [] as discovery}
						<li class="flex flex-col gap-2 py-3 first:pt-0 last:pb-0 sm:flex-row sm:items-center sm:justify-between">
							<div class="min-w-0">
								<p class="truncate text-sm font-medium text-ink-200" title={discovery.source_path}>{discovery.source_path}</p>
								<p class="mt-1 text-xs text-ink-500">
									{m.duplicates_exact_discovery_match({ title: discovery.matched_book_title, format: discovery.matched_book_format.toUpperCase() })}
								</p>
							</div>
							<div class="flex shrink-0 items-center gap-2">
								<Badge variant="outline">{discovery.source_kind === 'upload' ? m.duplicates_exact_discovery_upload() : m.duplicates_exact_discovery_library_scan()}</Badge>
								<span class="text-xs tabular-nums text-ink-500">{m.duplicates_exact_discovery_seen({ count: formatNumber(discovery.seen_count) })}</span>
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		</Card.Content>
	</Card.Root>

	{#snippet candidateList(showSemanticNotice: boolean)}
		<div class="flex flex-col gap-4">
			{#if showSemanticNotice}
				<Alert.Root>
					<ShieldAlertIcon />
					<Alert.Title>{m.duplicates_semantic_review_title()}</Alert.Title>
					<Alert.Description>{m.duplicates_semantic_review_description()}</Alert.Description>
				</Alert.Root>
			{/if}
			<div class="flex flex-col gap-3 lg:flex-row lg:items-end lg:justify-between">
				<div>
					<h2 id="pair-list-title" class="text-lg font-semibold text-ink-100">{m.duplicates_candidates_title()}</h2>
					<p class="mt-1 text-sm text-ink-500">
						{m.duplicates_candidates_description()}
						{#if (pairs.data?.total ?? 0) > 0}{m.duplicates_range({ start: pairRangeStart, end: pairRangeEnd, total: formatNumber(pairs.data?.total ?? 0) })}{/if}
					</p>
				</div>
				<div class="flex flex-col gap-2 sm:flex-row">
					{#if !showSemanticNotice}
						<Select.Root type="single" bind:value={relationFilter}>
							<Select.Trigger aria-label={m.duplicates_relation_filter_aria()} class="w-full sm:w-48">{relationOptions.find((option) => option.value === relationFilter)?.label}</Select.Trigger>
							<Select.Content>
								<Select.Group>
									{#each relationOptions as option}<Select.Item value={option.value}>{option.label}</Select.Item>{/each}
								</Select.Group>
							</Select.Content>
						</Select.Root>
					{/if}
					<Select.Root type="single" bind:value={statusFilter}>
						<Select.Trigger aria-label={m.duplicates_status_filter_aria()} class="w-full sm:w-40">{statusOptions.find((option) => option.value === statusFilter)?.label}</Select.Trigger>
						<Select.Content>
							<Select.Group>
								{#each statusOptions as option}<Select.Item value={option.value}>{option.label}</Select.Item>{/each}
							</Select.Group>
						</Select.Content>
					</Select.Root>
				</div>
			</div>

			{#if pairs.isLoading}
				<div class="grid gap-4">
					{#each Array(3) as _}<Skeleton class="h-72 w-full" />{/each}
				</div>
			{:else if pairs.isError}
				<Card.Root>
					<Card.Header>
						<Card.Title>{m.duplicates_candidates_load_failed()}</Card.Title>
						<Card.Description>{m.duplicates_candidates_load_failed_hint({ error: getErrorMessage(pairs.error) })}</Card.Description>
						<Card.Action><Button variant="outline" onclick={() => pairs.refetch()}>{m.duplicates_reload()}</Button></Card.Action>
					</Card.Header>
				</Card.Root>
			{:else if pairItems.length === 0}
				<Card.Root>
					<Card.Header class="items-center text-center">
						<div class="mb-2 flex size-10 items-center justify-center rounded-full bg-ink-800"><CheckCircleIcon class="size-5 text-ink-300" /></div>
						<Card.Title>{m.duplicates_candidates_empty()}</Card.Title>
						<Card.Description>{m.duplicates_candidates_empty_hint()}</Card.Description>
					</Card.Header>
				</Card.Root>
			{:else}
				<div class="grid gap-4">
					{#each pairItems as pair (pair.id)}
						<DuplicatePairCard {pair} onview={openPair} />
					{/each}
				</div>
				{#if (pairs.data?.total ?? 0) > PAIR_PAGE_SIZE}
					<nav class="flex items-center justify-between gap-3" aria-label={m.duplicates_candidates_pagination_aria()}>
						<Button variant="outline" onclick={() => pairPage = Math.max(0, pairPage - 1)} disabled={pairPage === 0 || pairs.isFetching}>{m.duplicates_previous_page()}</Button>
						<span class="text-xs tabular-nums text-ink-500">{m.duplicates_page_count({ page: pairPage + 1, total: Math.ceil((pairs.data?.total ?? 0) / PAIR_PAGE_SIZE) })}</span>
						<Button variant="outline" onclick={() => pairPage += 1} disabled={pairRangeEnd >= (pairs.data?.total ?? 0) || pairs.isFetching}>{m.duplicates_next_page()}</Button>
					</nav>
				{/if}
			{/if}
		</div>
	{/snippet}

	<section aria-labelledby="pair-list-title">
		<Tabs.Root value={candidateKind} onValueChange={changeCandidateKind}>
			<Tabs.List aria-label={m.duplicates_candidate_kind_aria()} class="w-full justify-start sm:w-fit">
				<Tabs.Trigger value="content" class="px-3">{m.duplicates_content_candidates_tab()}</Tabs.Trigger>
				<Tabs.Trigger value="semantic" class="px-3">{m.duplicates_semantic_candidates_tab()}</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="content" class="pt-2">
				{#if candidateKind === 'content'}{@render candidateList(false)}{/if}
			</Tabs.Content>
			<Tabs.Content value="semantic" class="pt-2">
				{#if candidateKind === 'semantic'}{@render candidateList(true)}{/if}
			</Tabs.Content>
		</Tabs.Root>
	</section>
</div>

<Sheet.Root bind:open={detailOpen}>
	<Sheet.Content class="overflow-y-auto data-[side=right]:w-full data-[side=right]:sm:max-w-3xl">
		<Sheet.Header>
			<Sheet.Title>{m.duplicates_detail_title()}</Sheet.Title>
			<Sheet.Description>{m.duplicates_detail_description()}</Sheet.Description>
		</Sheet.Header>

		{#if pairDetail.isLoading || !selectedPair}
			<div class="flex flex-col gap-4 px-4"><Skeleton class="h-20 w-full" /><Skeleton class="h-48 w-full" /></div>
		{:else if pairDetail.isError}
			<div class="mx-4 flex items-start gap-2 rounded-lg p-4 text-sm text-destructive ring-1 ring-destructive/20">
				<AlertTriangleIcon class="mt-0.5 size-4 shrink-0" />
				<div><p class="font-medium">{m.duplicates_evidence_load_failed()}</p><p class="mt-1">{getErrorMessage(pairDetail.error)}</p></div>
			</div>
		{:else}
			<Tabs.Root value="evidence" class="min-h-0 flex-1 px-4 pb-4">
				<Tabs.List variant="line">
					<Tabs.Trigger value="evidence">{m.duplicates_evidence_tab()}</Tabs.Trigger>
					<Tabs.Trigger value="resolve">{m.duplicates_resolve_tab()}</Tabs.Trigger>
				</Tabs.List>

				<Tabs.Content value="evidence" class="flex flex-col gap-5 pt-4">
					<div class="grid gap-3 sm:grid-cols-2">
						{#each [['A', selectedPair.book_a], ['B', selectedPair.book_b]] as item}
							{@const [side, book] = item as ['A' | 'B', DuplicateBookSummary]}
							<div class="rounded-lg bg-ink-950/45 p-3 ring-1 ring-ink-800/70">
								<p class="text-xs font-medium text-ink-500">{m.duplicates_version_label({ side })}</p>
								<a href={`/library/${book.id}`} class="mt-1 block font-semibold text-ink-100 hover:text-accent-400">{book.title}</a>
								<p class="mt-1 text-xs text-ink-500">{m.duplicates_book_stats({ chapters: formatNumber(book.chapter_count), words: formatNumber(book.word_count) })}</p>
							</div>
						{/each}
					</div>

					{#if selectedPair.relation === 'semantic_relation'}
						<SemanticEvidencePanel pair={selectedPair} />
						{:else}
							<PrimaryRecommendationPanel pair={selectedPair} />
							<GroupedAlignmentPanel pair={selectedPair} />
						<div class="rounded-lg bg-ink-950/30 p-4 ring-1 ring-ink-800/60">
							<div class="mb-4 flex flex-wrap gap-4 text-xs text-ink-400">
								<span>{m.duplicates_shared_chapters({ count: selectedPair.shared_chapters })}</span>
								<span>{m.duplicates_longest_run({ count: selectedPair.longest_contiguous_run })}</span>
								<span>{m.duplicates_order_score({ score: Math.round(selectedPair.order_score * 100) })}</span>
							</div>
							<ChapterOverlapTrack
								pair={selectedPair}
								matchedIndicesA={pairDetail.data?.matched_indices_a}
								matchedIndicesB={pairDetail.data?.matched_indices_b}
							/>
						</div>

						<div>
							<div class="flex flex-wrap items-center justify-between gap-2">
								<h3 class="text-sm font-semibold text-ink-200">{m.duplicates_chapter_matches_title()}</h3>
								{#if matchTotal > 0}<span class="text-xs tabular-nums text-ink-500">{m.duplicates_range({ start: matchRangeStart, end: matchRangeEnd, total: formatNumber(matchTotal) })}</span>{/if}
							</div>
							<div class="mt-3 flex max-h-[42vh] flex-col gap-2 overflow-y-auto pr-1">
								{#each matchItems as match (match.id)}
									<div class="rounded-lg p-3 text-xs ring-1 ring-ink-800/70">
										<div class="grid gap-2 sm:grid-cols-[1fr_auto_1fr] sm:items-center">
											<span class="text-ink-300">{chapterLabel(match.chapter_a_index, match.chapter_a_title)}</span>
											<Badge variant="outline">{Math.round(match.similarity * 100)}%</Badge>
											<span class="text-ink-300 sm:text-right">{chapterLabel(match.chapter_b_index, match.chapter_b_title)}</span>
										</div>
										<ChapterMatchDiff pairId={selectedPair.id} matchId={match.id} />
									</div>
								{:else}
									<p class="text-sm text-ink-500">{m.duplicates_chapter_matches_empty()}</p>
								{/each}
							</div>
							{#if matchTotal > MATCH_PAGE_SIZE}
								<nav class="mt-3 flex items-center justify-between gap-3" aria-label={m.duplicates_chapter_pagination_aria()}>
									<Button size="sm" variant="outline" onclick={() => matchPage = Math.max(0, matchPage - 1)} disabled={matchPage === 0 || pairDetail.isFetching}>{m.duplicates_previous_page()}</Button>
									<span class="text-xs tabular-nums text-ink-500">{m.duplicates_page_count({ page: matchPage + 1, total: Math.ceil(matchTotal / MATCH_PAGE_SIZE) })}</span>
									<Button size="sm" variant="outline" onclick={() => matchPage += 1} disabled={matchRangeEnd >= matchTotal || pairDetail.isFetching}>{m.duplicates_next_page()}</Button>
								</nav>
							{/if}
						</div>
					{/if}
				</Tabs.Content>

				<Tabs.Content value="resolve" class="flex flex-col gap-4 pt-4">
					<div class="rounded-lg bg-ink-950/40 p-4 ring-1 ring-ink-800/70">
						<h3 class="text-sm font-semibold text-ink-100">{m.duplicates_resolve_title()}</h3>
						<p class="mt-1 text-sm leading-6 text-ink-500">{m.duplicates_resolve_description()}</p>
					</div>

					<div class="grid gap-3 sm:grid-cols-2">
						<Button variant={selectedPair.recommended_primary_id === selectedPair.book_a.id ? 'default' : 'outline'} onclick={() => requestKeep('keep_a')}>
							{m.duplicates_keep_version({ side: 'A', title: selectedPair.book_a.title })}
						</Button>
						<Button variant={selectedPair.recommended_primary_id === selectedPair.book_b.id ? 'default' : 'outline'} onclick={() => requestKeep('keep_b')}>
							{m.duplicates_keep_version({ side: 'B', title: selectedPair.book_b.title })}
						</Button>
					</div>

					<Separator />

					<div class="flex flex-col gap-2 sm:flex-row sm:flex-wrap">
						<Button variant="secondary" onclick={() => submitResolution('same_work')} disabled={resolvePair.isPending}>{m.duplicates_same_work()}</Button>
						<Button variant="outline" onclick={() => submitResolution('defer')} disabled={resolvePair.isPending}>{m.duplicates_defer()}</Button>
						<Button variant="ghost" onclick={() => submitResolution('dismiss')} disabled={resolvePair.isPending}>{m.duplicates_dismiss()}</Button>
					</div>
				</Tabs.Content>
			</Tabs.Root>
		{/if}
	</Sheet.Content>
</Sheet.Root>

<Dialog.Root bind:open={confirmOpen}>
	<Dialog.Content class="sm:max-w-md">
		<Dialog.Header>
			<div class="mb-2 flex size-10 items-center justify-center rounded-full bg-destructive/10 text-destructive"><ShieldAlertIcon class="size-5" /></div>
			<Dialog.Title>{m.duplicates_confirm_keep_title({ book: bookLabel(confirmPrimary) })}</Dialog.Title>
			<Dialog.Description>
				{m.duplicates_confirm_keep_description({ book: bookLabel(confirmSecondary) })}
			</Dialog.Description>
		</Dialog.Header>
		<Dialog.Footer>
			<Button variant="outline" onclick={() => confirmOpen = false} disabled={resolvePair.isPending}>{m.common_cancel()}</Button>
			<Button variant="destructive" onclick={() => confirmAction && submitResolution(confirmAction)} disabled={resolvePair.isPending}>
				{m.duplicates_confirm_keep_action()}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
