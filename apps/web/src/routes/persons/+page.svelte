<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import { api } from '$services/api';
	import { User, BookOpen, Search, Filter } from 'lucide-svelte';

	let searchQuery = $state('');
	let roleFilter = $state('all');

	const persons = createQuery(() => ({
		queryKey: ['persons', searchQuery, roleFilter],
		queryFn: () => api.getPersons({ search: searchQuery || undefined, role: roleFilter === 'all' ? undefined : roleFilter }),
	}));

	const roles: Array<{ value: string; label: string }> = [
		{ value: 'all', label: '全部' },
		{ value: 'author', label: '作者' },
		{ value: 'translator', label: '译者' },
		{ value: 'editor', label: '编辑' },
		{ value: 'illustrator', label: '插画师' },
	];
</script>

<svelte:head>
	<title>Nova Reader — 作者管理</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">作者 & 创作者</h1>
			<p class="mt-1 text-sm text-ink-400">管理书籍关联的作者、译者和编辑</p>
		</div>
	</div>

	<!-- Filters -->
	<div class="flex items-center gap-4">
		<!-- Search -->
		<div class="relative flex-1 max-w-sm">
			<Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-ink-500" />
			<input
				type="text"
				bind:value={searchQuery}
				aria-label="搜索创作者"
				name="person-search"
				autocomplete="off"
				placeholder="搜索作者名…"
				class="w-full pl-9 pr-3 py-2 rounded-lg bg-ink-900 border border-ink-800 text-sm text-ink-100 placeholder:text-ink-600 focus:border-amber-500/50 focus:outline-none"
			/>
		</div>

		<!-- Role Filter -->
		<div class="flex items-center gap-1 rounded-lg bg-ink-900 border border-ink-800 p-1">
			{#each roles as role}
				<button
					class="px-3 py-1.5 text-xs font-medium rounded-md transition-colors {roleFilter === role.value ? 'bg-amber-500/10' : ''}"
					class:text-amber-300={roleFilter === role.value}
					class:text-ink-400={roleFilter !== role.value}
					class:hover:text-ink-200={roleFilter !== role.value}
					onclick={() => roleFilter = role.value}
				>
					{role.label}
				</button>
			{/each}
		</div>
	</div>

	<!-- Person Grid -->
	{#if persons.isLoading}
		<div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
			{#each Array(6) as _}
				<div class="h-40 rounded-xl bg-ink-900 border border-ink-800 animate-pulse"></div>
			{/each}
		</div>
	{:else if persons.data && persons.data.length > 0}
		<div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
			{#each persons.data as person}
					<a
						href="/persons/{person.id}"
						class="group rounded-xl border border-ink-800/50 bg-ink-900/80 p-5 transition-colors hover:border-amber-500/30 hover:bg-ink-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-400/70"
					>
					<div class="flex items-start gap-4">
						<!-- Avatar -->
						<div class="shrink-0 w-12 h-12 rounded-full bg-gradient-to-br from-amber-500/20 to-ink-800 flex items-center justify-center">
							{#if person.avatar_path}
								<img src={person.avatar_path} alt={person.name} width="48" height="48" loading="lazy" class="w-full h-full rounded-full object-cover" />
							{:else}
								<User class="w-5 h-5 text-ink-400" />
							{/if}
						</div>

						<div class="flex-1 min-w-0">
							<h3 class="text-sm font-semibold text-ink-100 group-hover:text-amber-300 transition-colors truncate">
								{person.name}
							</h3>
							{#if person.original_name && person.original_name !== person.name}
								<p class="text-xs text-ink-500 truncate">{person.original_name}</p>
							{/if}

							<!-- Role badges -->
							<div class="mt-1.5 flex flex-wrap gap-1">
								{#each person.roles as role}
									<span class="inline-flex items-center rounded-md bg-ink-700/50 px-1.5 py-0.5 text-[10px] text-ink-300">
										{role === 'author' ? '作者' : role === 'translator' ? '译者' : role === 'editor' ? '编辑' : role === 'illustrator' ? '插画' : role}
									</span>
								{/each}
							</div>
						</div>
					</div>

					<!-- Stats -->
					<div class="mt-4 flex items-center gap-4 text-xs text-ink-500">
						<span class="flex items-center gap-1">
							<BookOpen class="w-3 h-3" />
							{person.book_count} 本
						</span>
						{#if person.total_word_count > 0}
							<span>
								{person.total_word_count >= 10000
									? `${(person.total_word_count / 10000).toFixed(1)}万字`
									: `${person.total_word_count}字`}
							</span>
						{/if}
					</div>
				</a>
			{/each}
		</div>
	{:else}
		<div class="text-center py-16">
			<User class="w-12 h-12 text-ink-700 mx-auto mb-4" />
			<p class="text-ink-400">暂无作者数据</p>
			<p class="text-sm text-ink-600 mt-1">导入书籍后，系统会自动提取作者信息</p>
		</div>
	{/if}
</div>
