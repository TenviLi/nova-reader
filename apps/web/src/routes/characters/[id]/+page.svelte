<script lang="ts">
	import { page } from '$app/stores';
	import { api } from '$services/api';
	import { toast } from 'svelte-sonner';
	import type { Entity } from '$types/models';
	import { ChevronLeft, User } from 'lucide-svelte';

	let entity = $state<Entity | null>(null);
	let relations = $state<Array<{ source: string; target: string; type: string; targetName: string }>>([]);
	let mentions = $state<Array<{ book_title: string; chapter_title: string; context: string; chapter_index: number; book_id: string }>>([]);
	let timeline = $state<Array<{ chapter_index: number; chapter_title: string | null; context: string | null; position?: number }>>([]);
	let loading = $state(true);
	let activeTab = $state<'overview' | 'relations' | 'mentions' | 'timeline' | 'profile'>('overview');
	let generatingProfile = $state(false);

	let profile = $state<{
		appearance: string | null;
		personality: string | null;
		background: string | null;
		abilities: string | null;
		motivation: string | null;
		arc_summary: string | null;
		attributes: Record<string, string>;
		confidence_score: number;
		last_updated_by?: string;
	} | null>(null);

	const entityId = $derived($page.params.id!);

	$effect(() => {
		if (entityId) loadEntity(entityId);
	});

	async function loadEntity(id: string) {
		loading = true;
		try {
			const [entityData, relData, mentionData, timelineData] = await Promise.all([
				api.getEntity(id),
				api.getEntityRelations(id),
				api.getEntityMentions(id),
				api.getEntityTimeline(id).catch(() => []),
			]);
			entity = entityData;
			relations = relData;
			mentions = mentionData;
			timeline = timelineData;

			// Try to load existing profile (non-blocking)
			api.getEntityProfile(id).then(p => {
				if (p.last_updated_by !== 'none') {
					profile = p;
				}
			}).catch(() => { /* No profile yet */ });
		} finally {
			loading = false;
		}
	}

	async function generateProfile() {
		if (!entityId) return;
		generatingProfile = true;
		try {
			const result = await api.generateEntityProfile(entityId);
			profile = result;
			toast.success('角色档案已生成');
		} catch (err) {
			const msg = err instanceof Error ? err.message : '生成失败';
			toast.error('档案生成失败', { description: msg });
		} finally {
			generatingProfile = false;
		}
	}

	const entityTypeColors: Record<string, string> = {
		person: 'bg-entity-person/10 text-entity-person border-entity-person/30',
		location: 'bg-entity-location/10 text-entity-location border-entity-location/30',
		organization: 'bg-entity-organization/10 text-entity-organization border-entity-organization/30',
		item: 'bg-entity-item/10 text-entity-item border-entity-item/30',
		event: 'bg-entity-event/10 text-entity-event border-entity-event/30',
		concept: 'bg-accent-100 text-accent-700 border-accent-300',
	};

	const entityTypeLabels: Record<string, string> = {
		person: '人物',
		location: '地点',
		organization: '组织',
		item: '物品',
		event: '事件',
		concept: '概念',
	};
</script>

<svelte:head>
	<title>{entity?.name ?? '角色'} - Nova Reader</title>
</svelte:head>

<div class="p-6 max-w-5xl mx-auto">
	{#if loading}
		<div class="animate-pulse space-y-6">
			<div class="h-8 w-48 bg-parchment-200 rounded"></div>
			<div class="h-4 w-96 bg-parchment-100 rounded"></div>
			<div class="h-64 bg-parchment-100 rounded-xl"></div>
		</div>
	{:else if entity}
		<!-- Header -->
		<div class="mb-8">
			<div class="flex items-center gap-3 mb-2">
				<a href="/characters" class="text-ink-400 hover:text-ink-600 transition-colors">
					<ChevronLeft size={20} strokeWidth={2} />
				</a>
				<span class="px-2.5 py-0.5 text-xs font-medium rounded-full border {entityTypeColors[entity.entity_type ?? entity.type] ?? 'bg-ink-100 text-ink-600 border-ink-200'}">
					{entityTypeLabels[entity.entity_type ?? entity.type] ?? entity.entity_type ?? entity.type}
				</span>
			</div>
			<h1 class="text-3xl font-bold text-ink-900">{entity.name}</h1>
			{#if entity.aliases?.length}
				<p class="text-ink-500 mt-1">
					别名：{entity.aliases.join('、')}
				</p>
			{/if}
			{#if entity.description}
				<p class="text-ink-600 mt-3 leading-relaxed">{entity.description}</p>
			{/if}
		</div>

		<!-- Tabs -->
		<div class="border-b border-parchment-200 mb-6">
			<nav class="flex gap-6">
				{#each [
					{ id: 'overview', label: '概览' },
					{ id: 'profile', label: 'AI 档案' },
					{ id: 'relations', label: '关系' },
					{ id: 'mentions', label: '出现' },
					{ id: 'timeline', label: '时间线' },
				] as tab}
					<button
						onclick={() => activeTab = tab.id as typeof activeTab}
						class="pb-3 text-sm font-medium border-b-2 transition-colors {activeTab === tab.id ? 'border-accent-500 text-accent-700' : 'border-transparent text-ink-500 hover:text-ink-700'}"
					>
						{tab.label}
					</button>
				{/each}
			</nav>
		</div>

		<!-- Tab Content -->
		{#if activeTab === 'overview'}
			<div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
				<!-- Stats -->
				<div class="lg:col-span-2 space-y-6">
					<div class="grid grid-cols-3 gap-4">
						<div class="p-4 bg-parchment-50 rounded-xl text-center">
							<div class="text-2xl font-bold text-ink-800">{mentions.length}</div>
							<div class="text-xs text-ink-500 mt-1">出现次数</div>
						</div>
						<div class="p-4 bg-parchment-50 rounded-xl text-center">
							<div class="text-2xl font-bold text-ink-800">{relations.length}</div>
							<div class="text-xs text-ink-500 mt-1">关系数</div>
						</div>
						<div class="p-4 bg-parchment-50 rounded-xl text-center">
							<div class="text-2xl font-bold text-ink-800">
								{new Set(mentions.map(m => m.book_id)).size}
							</div>
							<div class="text-xs text-ink-500 mt-1">出现书目</div>
						</div>
					</div>

					<!-- Recent mentions -->
					<div>
						<h3 class="text-sm font-medium text-ink-700 mb-3">最近出现</h3>
						<div class="space-y-2">
							{#each mentions.slice(0, 5) as mention}
								<a
									href="/reading/{mention.book_id}?chapter={mention.chapter_index}"
									class="block p-3 bg-white border border-parchment-200 rounded-lg hover:border-accent-300 transition-colors"
								>
									<div class="text-xs text-ink-400 mb-1">
										{mention.book_title} · {mention.chapter_title}
									</div>
									<p class="text-sm text-ink-600 line-clamp-2">{mention.context}</p>
								</a>
							{/each}
						</div>
					</div>
				</div>

				<!-- Relationship mini-graph -->
				<div class="p-4 bg-parchment-50 rounded-xl">
					<h3 class="text-sm font-medium text-ink-700 mb-3">关系网络</h3>
					<div class="h-64 flex items-center justify-center text-ink-400 text-sm">
						<div class="text-center space-y-3">
							{#each relations.slice(0, 6) as rel}
								<div class="flex items-center gap-2 text-xs">
									<span class="text-ink-700 font-medium">{entity.name}</span>
									<span class="px-1.5 py-0.5 bg-parchment-200 rounded text-ink-500">{rel.type}</span>
									<span class="text-ink-700">{rel.targetName}</span>
								</div>
							{/each}
							{#if relations.length === 0}
								<p class="text-ink-400">暂无关系数据</p>
							{/if}
						</div>
					</div>
				</div>
			</div>

		{:else if activeTab === 'profile'}
			<div class="space-y-6">
				{#if profile}
					<!-- Confidence indicator -->
					<div class="flex items-center gap-2 text-sm text-ink-500">
						<div class="h-2 w-24 rounded-full bg-parchment-200 overflow-hidden">
							<div class="h-full bg-accent-500 rounded-full" style="width: {profile.confidence_score * 100}%"></div>
						</div>
						<span>可信度 {Math.round(profile.confidence_score * 100)}%</span>
						<span class="text-ink-400">· {profile.last_updated_by === 'ai' ? 'AI 生成' : '人工编辑'}</span>
					</div>

					<div class="grid grid-cols-1 md:grid-cols-2 gap-4">
						{#if profile.appearance}
							<div class="p-4 bg-parchment-50 rounded-xl">
								<h4 class="text-xs font-medium text-ink-400 mb-2">外貌特征</h4>
								<p class="text-sm text-ink-700">{profile.appearance}</p>
							</div>
						{/if}
						{#if profile.personality}
							<div class="p-4 bg-parchment-50 rounded-xl">
								<h4 class="text-xs font-medium text-ink-400 mb-2">性格特征</h4>
								<p class="text-sm text-ink-700">{profile.personality}</p>
							</div>
						{/if}
						{#if profile.background}
							<div class="p-4 bg-parchment-50 rounded-xl">
								<h4 class="text-xs font-medium text-ink-400 mb-2">背景故事</h4>
								<p class="text-sm text-ink-700">{profile.background}</p>
							</div>
						{/if}
						{#if profile.abilities}
							<div class="p-4 bg-parchment-50 rounded-xl">
								<h4 class="text-xs font-medium text-ink-400 mb-2">能力/技能</h4>
								<p class="text-sm text-ink-700">{profile.abilities}</p>
							</div>
						{/if}
						{#if profile.motivation}
							<div class="p-4 bg-parchment-50 rounded-xl md:col-span-2">
								<h4 class="text-xs font-medium text-ink-400 mb-2">动机与目标</h4>
								<p class="text-sm text-ink-700">{profile.motivation}</p>
							</div>
						{/if}
						{#if profile.arc_summary}
							<div class="p-4 bg-parchment-50 rounded-xl md:col-span-2">
								<h4 class="text-xs font-medium text-ink-400 mb-2">角色弧光</h4>
								<p class="text-sm text-ink-700">{profile.arc_summary}</p>
							</div>
						{/if}
					</div>

					<!-- Attributes table -->
					{#if Object.keys(profile.attributes).length > 0}
						<div class="p-4 bg-parchment-50 rounded-xl">
							<h4 class="text-xs font-medium text-ink-400 mb-3">属性</h4>
							<div class="grid grid-cols-2 md:grid-cols-4 gap-2">
								{#each Object.entries(profile.attributes) as [key, value]}
									<div class="text-sm">
										<span class="text-ink-400">{key}:</span>
										<span class="text-ink-700 font-medium ml-1">{value}</span>
									</div>
								{/each}
							</div>
						</div>
					{/if}

					<button
						onclick={generateProfile}
						disabled={generatingProfile}
						class="text-sm text-accent-600 hover:text-accent-500 disabled:opacity-50"
					>
						{generatingProfile ? '重新生成中...' : '🔄 重新生成档案'}
					</button>
				{:else}
					<div class="text-center py-12">
						<div class="mb-4 flex h-16 w-16 mx-auto items-center justify-center rounded-2xl bg-parchment-100">
							<User size={32} strokeWidth={1.5} class="text-ink-400" />
						</div>
						<h3 class="text-lg font-medium text-ink-700">尚无 AI 档案</h3>
						<p class="mt-1 text-sm text-ink-500">让 AI 分析所有出现片段，自动生成角色画像</p>
						<button
							onclick={generateProfile}
							disabled={generatingProfile}
							class="mt-4 rounded-lg bg-accent-600 px-4 py-2 text-sm font-medium text-white hover:bg-accent-500 disabled:opacity-50 transition-colors"
						>
							{#if generatingProfile}
								<span class="inline-flex items-center gap-2">
									<span class="h-3.5 w-3.5 animate-spin rounded-full border-2 border-white border-t-transparent"></span>
									生成中...
								</span>
							{:else}
								✨ 生成 AI 角色档案
							{/if}
						</button>
					</div>
				{/if}
			</div>

		{:else if activeTab === 'relations'}
			<div class="space-y-3">
				{#if relations.length === 0}
					<p class="text-center text-ink-400 py-12">暂无关系数据</p>
				{:else}
					{#each relations as rel}
						<div class="flex items-center gap-4 p-4 bg-white border border-parchment-200 rounded-lg">
							<div class="flex-1">
								<span class="font-medium text-ink-800">{entity.name}</span>
							</div>
							<div class="px-3 py-1 bg-accent-50 text-accent-700 rounded-full text-xs font-medium">
								{rel.type}
							</div>
							<div class="flex-1 text-right">
								<a href="/characters/{rel.target}" class="font-medium text-ink-800 hover:text-accent-600">
									{rel.targetName}
								</a>
							</div>
						</div>
					{/each}
				{/if}
			</div>

		{:else if activeTab === 'mentions'}
			<div class="space-y-2">
				{#if mentions.length === 0}
					<p class="text-center text-ink-400 py-12">暂未找到出现记录</p>
				{:else}
					{#each mentions as mention}
						<a
							href="/reading/{mention.book_id}?chapter={mention.chapter_index}"
							class="block p-4 bg-white border border-parchment-200 rounded-lg hover:border-accent-300 transition-colors"
						>
							<div class="flex items-center gap-2 text-xs text-ink-400 mb-2">
								<span class="font-medium text-ink-600">{mention.book_title}</span>
								<span>·</span>
								<span>{mention.chapter_title}</span>
							</div>
							<p class="text-sm text-ink-700">{mention.context}</p>
						</a>
					{/each}
				{/if}
			</div>

		{:else if activeTab === 'timeline'}
			<div class="relative pl-8 space-y-6">
				<div class="absolute left-3 top-2 bottom-2 w-px bg-parchment-300"></div>
				{#if timeline.length === 0}
					<p class="text-center text-ink-400 py-12">暂无时间线数据</p>
				{:else}
					{#each timeline as event, i}
						<div class="relative">
							<div class="absolute -left-5 top-1.5 w-3 h-3 rounded-full border-2 border-accent-500 bg-white"></div>
							<a
								href={mentions.find((mention) => mention.chapter_index === event.chapter_index)?.book_id ? `/reading/${mentions.find((mention) => mention.chapter_index === event.chapter_index)?.book_id}?chapter=${event.chapter_index}` : undefined}
								class="block p-3 bg-parchment-50 rounded-lg transition-colors hover:bg-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400"
							>
								<div class="text-xs text-ink-400 mb-1">
									第 {event.chapter_index + 1} 章 · {event.chapter_title ?? '未命名章节'}
									{#if typeof event.position === 'number'}
										<span class="ml-1">· {event.position}</span>
									{/if}
								</div>
								<p class="text-sm text-ink-600">{event.context ?? '暂无上下文'}</p>
							</a>
						</div>
					{/each}
				{/if}
			</div>
		{/if}
	{:else}
		<div class="text-center py-20">
			<p class="text-ink-500">未找到该角色</p>
			<a href="/characters" class="mt-4 inline-block text-accent-600 hover:underline">返回角色列表</a>
		</div>
	{/if}
</div>
