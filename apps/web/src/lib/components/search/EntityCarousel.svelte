<script lang="ts">
	import { ChevronLeft, ChevronRight, User, MapPin, Zap, Calendar } from 'lucide-svelte';

	interface Entity {
		id: string;
		name: string;
		type: 'character' | 'place' | 'ability' | 'event';
		description?: string;
		mention_count: number;
		books: string[];
	}

	interface Props {
		entities: Entity[];
		onEntityClick?: (entity: Entity) => void;
	}

	let { entities = [], onEntityClick }: Props = $props();

	let scrollContainer = $state<HTMLElement | null>(null);
	let canScrollLeft = $state(false);
	let canScrollRight = $state(true);

	const TYPE_ICONS = {
		character: User,
		place: MapPin,
		ability: Zap,
		event: Calendar,
	};

	const TYPE_COLORS = {
		character: 'from-purple-500 to-purple-600',
		place: 'from-emerald-500 to-emerald-600',
		ability: 'from-red-500 to-red-600',
		event: 'from-amber-500 to-amber-600',
	};

	function handleScroll() {
		if (!scrollContainer) return;
		canScrollLeft = scrollContainer.scrollLeft > 0;
		canScrollRight = scrollContainer.scrollLeft < scrollContainer.scrollWidth - scrollContainer.clientWidth - 10;
	}

	function scroll(direction: 'left' | 'right') {
		if (!scrollContainer) return;
		const amount = direction === 'left' ? -280 : 280;
		scrollContainer.scrollBy({ left: amount, behavior: 'smooth' });
	}
</script>

{#if entities.length > 0}
	<div class="relative">
		<!-- Section header -->
		<div class="mb-3 flex items-center justify-between">
			<h4 class="text-sm font-medium text-ink-600 dark:text-ink-400">相关实体</h4>
			<div class="flex gap-1">
				<button
					class="flex h-7 w-7 items-center justify-center rounded-full border border-ink-200 transition-colors hover:bg-ink-50 disabled:opacity-30 dark:border-ink-600 dark:hover:bg-ink-800"
					onclick={() => scroll('left')}
					disabled={!canScrollLeft}
					type="button"
					aria-label="向左滚动实体"
				>
					<ChevronLeft class="h-4 w-4" />
				</button>
				<button
					class="flex h-7 w-7 items-center justify-center rounded-full border border-ink-200 transition-colors hover:bg-ink-50 disabled:opacity-30 dark:border-ink-600 dark:hover:bg-ink-800"
					onclick={() => scroll('right')}
					disabled={!canScrollRight}
					type="button"
					aria-label="向右滚动实体"
				>
					<ChevronRight class="h-4 w-4" />
				</button>
			</div>
		</div>

		<!-- Carousel -->
		<div
			bind:this={scrollContainer}
			onscroll={handleScroll}
			class="flex gap-3 overflow-x-auto scroll-smooth pb-2 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
		>
			{#each entities as entity (entity.id)}
				{@const Icon = TYPE_ICONS[entity.type]}
				<button
					class="group flex w-56 flex-shrink-0 flex-col rounded-xl border border-ink-100 bg-white p-4 text-left transition-all hover:border-accent-200 hover:shadow-md dark:border-ink-700 dark:bg-ink-900 dark:hover:border-accent-700"
					onclick={() => onEntityClick?.(entity)}
					type="button"
				>
					<!-- Icon + Type badge -->
					<div class="mb-3 flex items-center gap-2">
						<div class="flex h-8 w-8 items-center justify-center rounded-lg bg-gradient-to-br {TYPE_COLORS[entity.type]} text-white shadow-sm">
							<Icon class="h-4 w-4" />
						</div>
						<span class="rounded-full bg-ink-100 px-2 py-0.5 text-xs text-ink-500 dark:bg-ink-700">
							{entity.mention_count} 次提及
						</span>
					</div>

					<!-- Name -->
					<h5 class="mb-1 text-sm font-semibold text-ink-800 group-hover:text-accent-600 dark:text-ink-200 dark:group-hover:text-accent-400">
						{entity.name}
					</h5>

					<!-- Description -->
					{#if entity.description}
						<p class="mb-2 line-clamp-2 text-xs leading-relaxed text-ink-400">
							{entity.description}
						</p>
					{/if}

					<!-- Book count -->
					<div class="mt-auto text-xs text-ink-300">
						出现在 {entity.books.length} 本书中
					</div>
				</button>
			{/each}
		</div>
	</div>
{/if}
