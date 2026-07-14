<script lang="ts">
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import {
		Tags, Plus, Trash2, Sparkles, Radar, ShieldAlert,
		BookOpen, Play, Search, Bookmark as BookmarkIcon
	} from 'lucide-svelte';
	import type { BookRadarResult, RadarDataPoint, VibeBookmark, VibeSearchResult, SemanticOverview, SemanticProfile } from '$types/models';

	interface BookScore {
		tag_profile_id: string;
		name: string;
		color: string;
		category: string;
		is_warning: boolean;
		concentration: number;
		match_count: number;
		total_chunks: number;
		peak_chapter: number | null;
		peak_score: number | null;
	}

	let profiles = $state<SemanticProfile[]>([]);
	let overview = $state<SemanticOverview | null>(null);
	let loading = $state(false);
	let showCreateForm = $state(false);

	// Form state
	let newName = $state('');
	let newDescription = $state('');
	let newCategory = $state('custom');
	let newColor = $state('#6366f1');
	let newIsWarning = $state(false);
	let newThreshold = $state(0.45);
	let newReferenceTexts = $state<string[]>(['']);

	// Book scores view
	let books = $state<Array<{ id: string; title: string }>>([]);
	let selectedBookId = $state('');
	let bookScores = $state<BookScore[]>([]);
	let radarData = $state<BookRadarResult | null>(null);

	// Vibe search
	let vibeText = $state('');
	let vibeBookmarkName = $state('');
	let vibeBookmarks = $state<VibeBookmark[]>([]);
	let vibeResults = $state<VibeSearchResult[]>([]);
	let vibeSearching = $state(false);
	let vibeSaving = $state(false);

	type SemanticTab = 'profiles' | 'radar' | 'vibe';
	const initialTab = $page.url.searchParams.get('tab');
	let activeTab = $state<SemanticTab>(initialTab === 'radar' || initialTab === 'vibe' ? initialTab : 'profiles');

	const categories = [
		{ value: 'trope', label: '桥段 (Trope)' },
		{ value: 'emotion', label: '情感 (Emotion)' },
		{ value: 'setting', label: '设定 (Setting)' },
		{ value: 'warning', label: '雷区 (Warning)' },
		{ value: 'custom', label: '自定义' },
	];

	const presetColors = [
		'#6366f1', '#8b5cf6', '#ec4899', '#f43f5e',
		'#f97316', '#eab308', '#22c55e', '#06b6d4',
	];

	onMount(async () => {
		await Promise.all([
			loadProfiles(),
			loadOverview(),
			loadVibeBookmarks(),
		]);
		try {
			const result = await api.getBooks();
			books = result.data?.map((b) => ({ id: b.id, title: b.title })) ?? [];
		} catch { /* ignore */ }
	});

	async function loadProfiles() {
		loading = true;
		try {
			profiles = await api.getSemanticProfiles();
		} catch { profiles = []; }
		finally { loading = false; }
	}

	async function loadOverview() {
		try {
			overview = await api.getSemanticOverview();
		} catch {
			overview = null;
		}
	}

	function setActiveTab(tab: SemanticTab) {
		activeTab = tab;
		const url = new URL($page.url);
		if (tab === 'profiles') url.searchParams.delete('tab');
		else url.searchParams.set('tab', tab);
		goto(url.toString(), { replaceState: true, keepFocus: true, noScroll: true });
		if (tab === 'radar' && selectedBookId) loadBookScores();
	}

	async function createProfile() {
		const texts = newReferenceTexts.filter(t => t.trim());
		if (!newName.trim() || texts.length === 0) return;

		try {
			await api.createSemanticProfile({
				name: newName.trim(),
				description: newDescription || null,
				category: newCategory,
				color: newColor,
				reference_texts: texts,
				match_threshold: newThreshold,
				is_warning: newIsWarning,
			});
			resetForm();
			await loadProfiles();
		} catch (err) {
			console.error('Create failed', err);
		}
	}

	async function deleteProfile(id: string) {
		if (!confirm('确认删除此标签配置?')) return;
		try {
			await api.deleteSemanticProfile(id);
			await loadProfiles();
		} catch { /* ignore */ }
	}

	async function computeEmbedding(id: string) {
		try {
			await api.computeProfileEmbedding(id);
		} catch (err) {
			console.error('Embedding computation failed', err);
		}
	}

	async function triggerBookTagging() {
		if (!selectedBookId) return;
		try {
			await api.computeBookTags(selectedBookId);
		} catch { /* ignore */ }
	}

	async function loadBookScores() {
		if (!selectedBookId) return;
		try {
			const [scores, radar] = await Promise.all([
				api.getBookTagScores(selectedBookId),
				api.getBookRadar(selectedBookId),
			]);
			bookScores = scores;
			radarData = radar;
		} catch { bookScores = []; radarData = null; }
	}

	async function vibeSearch() {
		if (!vibeText.trim()) return;
		vibeSearching = true;
		try {
			const result = await api.vibeSearch({
				text: vibeText,
				limit: 20,
				threshold: 0.35,
			});
			vibeResults = result.results || [];
		} catch { vibeResults = []; }
		finally { vibeSearching = false; }
	}

	async function loadVibeBookmarks() {
		try {
			vibeBookmarks = await api.getVibeBookmarks();
		} catch {
			vibeBookmarks = [];
		}
	}

	function defaultVibeBookmarkName(): string {
		const text = vibeText.trim().replace(/\s+/g, ' ');
		return text.length > 18 ? `${text.slice(0, 18)}...` : text || '未命名氛围';
	}

	async function saveCurrentVibe() {
		if (!vibeText.trim() || vibeSaving) return;
		vibeSaving = true;
		try {
			const saved = await api.saveVibeBookmark({
				name: vibeBookmarkName.trim() || defaultVibeBookmarkName(),
				source_text: vibeText.trim(),
				source_book_id: null,
				source_chapter_index: null,
			});
			vibeBookmarks = [saved, ...vibeBookmarks.filter((bookmark) => bookmark.id !== saved.id)];
			vibeBookmarkName = '';
		} catch {
			// Keep the search workflow usable even if persistence fails.
		} finally {
			vibeSaving = false;
		}
	}

	async function runVibeBookmark(bookmark: VibeBookmark) {
		vibeText = bookmark.source_text;
		vibeBookmarkName = bookmark.name ?? '';
		await vibeSearch();
	}

	function resetForm() {
		showCreateForm = false;
		newName = '';
		newDescription = '';
		newCategory = 'custom';
		newColor = '#6366f1';
		newIsWarning = false;
		newThreshold = 0.45;
		newReferenceTexts = [''];
	}

	function addReferenceText() {
		newReferenceTexts = [...newReferenceTexts, ''];
	}

	function removeReferenceText(idx: number) {
		newReferenceTexts = newReferenceTexts.filter((_, i) => i !== idx);
	}

	function concentrationPercent(v: number): string {
		return (v * 100).toFixed(1) + '%';
	}

	$effect(() => {
		const tab = $page.url.searchParams.get('tab');
		if ((tab === 'radar' || tab === 'vibe') && tab !== activeTab) {
			activeTab = tab;
		} else if (!tab && activeTab !== 'profiles') {
			activeTab = 'profiles';
		}
	});

	// SVG radar chart generator
	function radarPath(axes: RadarDataPoint[]): string {
		if (!axes || axes.length < 3) return '';
		const cx = 150, cy = 150, r = 120;
		const n = axes.length;
		const points = axes.map((a, i) => {
			const angle = (Math.PI * 2 * i) / n - Math.PI / 2;
			const val = Math.max(0, Math.min(1, a.score));
			return `${cx + r * val * Math.cos(angle)},${cy + r * val * Math.sin(angle)}`;
		});
		return `M ${points.join(' L ')} Z`;
	}

	function radarGridPath(level: number, count: number): string {
		const cx = 150, cy = 150, r = 120;
		const points = Array.from({ length: count }, (_, i) => {
			const angle = (Math.PI * 2 * i) / count - Math.PI / 2;
			return `${cx + r * level * Math.cos(angle)},${cy + r * level * Math.sin(angle)}`;
		});
		return `M ${points.join(' L ')} Z`;
	}
</script>

<svelte:head>
	<title>Nova Reader — 智能标签</title>
</svelte:head>

<div class="mx-auto max-w-6xl px-6 py-6 space-y-6">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<div class="flex items-center gap-3">
			<Tags size={20} class="text-accent-400" />
			<h1 class="text-xl font-semibold text-ink-100">智能标签</h1>
			<span class="text-sm text-ink-500">标签画像 · 书籍浓度 · 与智能搜索联动</span>
		</div>
	</div>

	{#if overview}
		<section class="grid gap-3 sm:grid-cols-3" aria-label="智能标签概览">
			<div class="rounded-lg border border-ink-800/60 bg-ink-900/35 p-4">
				<p class="text-xs text-ink-500">标签画像</p>
				<p class="mt-1 text-xl font-semibold text-ink-100">{overview.total_profiles}</p>
			</div>
			<div class="rounded-lg border border-ink-800/60 bg-ink-900/35 p-4">
				<p class="text-xs text-ink-500">已标记书籍</p>
				<p class="mt-1 text-xl font-semibold text-ink-100">{overview.total_books_tagged}</p>
			</div>
			<div class="rounded-lg border border-ink-800/60 bg-ink-900/35 p-4">
				<p class="text-xs text-ink-500">主要分类</p>
				<p class="mt-1 truncate text-xl font-semibold text-ink-100">
					{Object.entries(overview.categories).sort((a, b) => b[1] - a[1])[0]?.[0] ?? '暂无'}
				</p>
			</div>
		</section>
	{/if}

	<!-- Tabs -->
	<div class="flex gap-1 rounded-lg bg-ink-900/30 p-1 w-fit border border-ink-800/50">
		<button
			type="button"
			class="px-4 py-1.5 rounded-md text-sm transition-colors {activeTab === 'profiles' ? 'bg-accent-500/20 text-accent-300' : 'text-ink-400 hover:text-ink-200'}"
			onclick={() => setActiveTab('profiles')}
			aria-pressed={activeTab === 'profiles'}
		>
			<Tags size={14} class="inline mr-1" /> 标签画像
		</button>
		<button
			type="button"
			class="px-4 py-1.5 rounded-md text-sm transition-colors {activeTab === 'radar' ? 'bg-accent-500/20 text-accent-300' : 'text-ink-400 hover:text-ink-200'}"
			onclick={() => setActiveTab('radar')}
			aria-pressed={activeTab === 'radar'}
		>
			<Radar size={14} class="inline mr-1" /> 书籍画像
		</button>
		<button
			type="button"
			class="px-4 py-1.5 rounded-md text-sm transition-colors {activeTab === 'vibe' ? 'bg-accent-500/20 text-accent-300' : 'text-ink-400 hover:text-ink-200'}"
			onclick={() => setActiveTab('vibe')}
			aria-pressed={activeTab === 'vibe'}
		>
			<Search size={14} class="inline mr-1" /> 氛围检索
		</button>
	</div>

	<!-- Tab: Tag Profiles -->
	{#if activeTab === 'profiles'}
		<div class="space-y-4">
			<!-- Create Button -->
			{#if !showCreateForm}
				<button
					type="button"
					onclick={() => showCreateForm = true}
					class="flex items-center gap-2 rounded-lg bg-accent-500/10 border border-accent-500/20 px-4 py-2 text-sm text-accent-400 hover:bg-accent-500/20 transition-colors"
				>
					<Plus size={14} /> 创建标签画像
				</button>
			{/if}

			<!-- Create Form -->
			{#if showCreateForm}
				<div class="rounded-xl border border-ink-700/50 bg-ink-900/50 p-6 space-y-4">
					<h3 class="text-lg font-medium text-ink-100">新建智能标签画像</h3>

					<div class="grid grid-cols-2 gap-4">
						<div>
							<label for="semantic-profile-name" class="block text-sm text-ink-400 mb-1">名称</label>
							<input
								id="semantic-profile-name"
								type="text"
								bind:value={newName}
								placeholder="例: 宗门政治、修炼瓶颈、师徒羁绊..."
								class="w-full rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-200 placeholder-ink-600 focus:border-accent-500/50 focus:outline-none"
							/>
						</div>
						<div>
							<label for="semantic-profile-category" class="block text-sm text-ink-400 mb-1">分类</label>
							<select
								id="semantic-profile-category"
								bind:value={newCategory}
								class="w-full rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-200 focus:border-accent-500/50 focus:outline-none"
							>
								{#each categories as cat}
									<option value={cat.value}>{cat.label}</option>
								{/each}
							</select>
						</div>
					</div>

					<div>
						<label for="semantic-profile-description" class="block text-sm text-ink-400 mb-1">描述</label>
						<input
							id="semantic-profile-description"
							type="text"
							bind:value={newDescription}
							placeholder="这个标签代表什么类型的情节..."
							class="w-full rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-200 placeholder-ink-600 focus:border-accent-500/50 focus:outline-none"
						/>
					</div>

					<!-- Color + Threshold -->
					<div class="flex items-center gap-4">
						<div>
							<p id="semantic-profile-color-label" class="block text-sm text-ink-400 mb-1">颜色</p>
							<div class="flex gap-1" role="group" aria-labelledby="semantic-profile-color-label">
								{#each presetColors as c}
									<button
										type="button"
										onclick={() => newColor = c}
										aria-label="选择颜色 {c}"
										aria-pressed={newColor === c}
										class="w-6 h-6 rounded-full border-2 transition-transform {newColor === c ? 'border-white scale-110' : 'border-transparent'}"
										style="background-color: {c}"
									></button>
								{/each}
							</div>
						</div>
						<div>
							<label for="semantic-profile-threshold" class="block text-sm text-ink-400 mb-1">匹配阈值: {newThreshold.toFixed(2)}</label>
							<input id="semantic-profile-threshold" type="range" min="0.2" max="0.8" step="0.05" bind:value={newThreshold} class="w-32" />
						</div>
						<label class="flex items-center gap-2 text-sm text-ink-400">
							<input type="checkbox" bind:checked={newIsWarning} class="rounded" />
							<ShieldAlert size={14} class="text-red-400" /> 标记为雷区
						</label>
					</div>

					<!-- Reference Texts -->
					<div>
						<p id="semantic-reference-texts-label" class="block text-sm text-ink-400 mb-1">
							参考文本（定义此标签的"氛围"）
						</p>
						<p class="text-xs text-ink-600 mb-2">粘贴典型的段落片段，系统将根据这些文本的语义相似度对小说进行打分</p>
						{#each newReferenceTexts as text, i}
							<div class="flex gap-2 mb-2">
								<textarea
									aria-labelledby="semantic-reference-texts-label"
									bind:value={newReferenceTexts[i]}
									rows="2"
									placeholder="粘贴一段典型的描述..."
									class="flex-1 rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-200 placeholder-ink-600 focus:border-accent-500/50 focus:outline-none resize-none"
								></textarea>
								{#if newReferenceTexts.length > 1}
									<button type="button" aria-label="删除第 {i + 1} 段参考文本" onclick={() => removeReferenceText(i)} class="text-red-400 hover:text-red-300">
										<Trash2 size={14} />
									</button>
								{/if}
							</div>
						{/each}
						<button type="button" onclick={addReferenceText} class="text-xs text-accent-400 hover:text-accent-300">
							+ 添加更多参考文本
						</button>
					</div>

					<!-- Actions -->
					<div class="flex gap-3">
						<button
							type="button"
							onclick={createProfile}
							class="rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-white hover:bg-accent-600 transition-colors"
						>
							创建
						</button>
						<button
							type="button"
							onclick={resetForm}
							class="rounded-lg border border-ink-700/50 px-4 py-2 text-sm text-ink-400 hover:text-ink-200 transition-colors"
						>
							取消
						</button>
					</div>
				</div>
			{/if}

			<!-- Profile List -->
			{#if loading}
				<div class="text-center py-8 text-ink-500">加载中...</div>
			{:else if profiles.length === 0}
				<div class="text-center py-12 text-ink-500">
					<Tags size={40} class="mx-auto mb-3 opacity-30" />
					<p>还没有标签画像</p>
					<p class="text-sm mt-1">创建画像后，系统会自动计算每本书的语义相似度</p>
				</div>
			{:else}
				<div class="grid gap-3">
					{#each profiles as profile}
						<div class="rounded-xl border border-ink-700/50 bg-ink-900/30 p-4 flex items-start gap-4">
							<!-- Color badge -->
							<div class="w-3 h-3 rounded-full mt-1.5 shrink-0" style="background-color: {profile.color}"></div>

							<div class="flex-1 min-w-0">
								<div class="flex items-center gap-2">
									<span class="font-medium text-ink-100">{profile.name}</span>
									{#if profile.is_warning}
										<span class="text-xs px-1.5 py-0.5 rounded bg-red-500/10 text-red-400 border border-red-500/20">雷区</span>
									{/if}
									<span class="text-xs px-1.5 py-0.5 rounded bg-ink-800/50 text-ink-500">{profile.category}</span>
								</div>
								{#if profile.description}
									<p class="text-sm text-ink-500 mt-1">{profile.description}</p>
								{/if}
								<p class="text-xs text-ink-600 mt-1">
									{profile.reference_texts.length} 个参考文本 · 阈值 {profile.match_threshold.toFixed(2)}
								</p>
							</div>

							<div class="flex gap-2 shrink-0">
								<button
									onclick={() => computeEmbedding(profile.id)}
									title="计算向量"
									class="p-1.5 rounded-lg text-ink-500 hover:text-accent-400 hover:bg-accent-500/10 transition-colors"
								>
									<Sparkles size={14} />
								</button>
								<button
									onclick={() => deleteProfile(profile.id)}
									title="删除"
									class="p-1.5 rounded-lg text-ink-500 hover:text-red-400 hover:bg-red-500/10 transition-colors"
								>
									<Trash2 size={14} />
								</button>
							</div>
						</div>
					{/each}
				</div>
			{/if}
		</div>
	{/if}

	<!-- Tab: Book Radar -->
	{#if activeTab === 'radar'}
		<div class="space-y-4">
			<div class="flex items-center gap-3">
				<select
					bind:value={selectedBookId}
					onchange={() => loadBookScores()}
					class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-4 py-2 text-sm text-ink-200 focus:border-accent-500/50 focus:outline-none min-w-[200px]"
				>
					<option value="">选择书籍...</option>
					{#each books as book}
						<option value={book.id}>{book.title}</option>
					{/each}
				</select>

				{#if selectedBookId}
					<button
						onclick={triggerBookTagging}
						class="flex items-center gap-2 rounded-lg bg-accent-500/10 border border-accent-500/20 px-4 py-2 text-sm text-accent-400 hover:bg-accent-500/20 transition-colors"
					>
						<Play size={14} /> 计算智能标签
					</button>
				{/if}
			</div>

			{#if radarData && radarData.axes?.length >= 3}
				<!-- Radar Chart -->
				<div class="rounded-xl border border-ink-700/50 bg-ink-900/30 p-6">
					<h3 class="text-sm font-medium text-ink-300 mb-4">标签雷达图</h3>
					<div class="flex justify-center">
						<svg viewBox="0 0 300 300" class="w-72 h-72">
							<!-- Grid -->
							{#each [0.25, 0.5, 0.75, 1.0] as level}
								<path d={radarGridPath(level, radarData.axes.length)} fill="none" stroke="currentColor" class="text-ink-800" stroke-width="0.5" />
							{/each}
							<!-- Axes -->
							{#each radarData.axes as _, i}
								{@const angle = (Math.PI * 2 * i) / radarData.axes.length - Math.PI / 2}
								<line x1="150" y1="150" x2={150 + 120 * Math.cos(angle)} y2={150 + 120 * Math.sin(angle)} stroke="currentColor" class="text-ink-800" stroke-width="0.5" />
							{/each}
							<!-- Data polygon -->
							<path d={radarPath(radarData.axes)} fill="rgba(99, 102, 241, 0.15)" stroke="#6366f1" stroke-width="2" />
							<!-- Labels -->
							{#each radarData.axes as axis, i}
								{@const angle = (Math.PI * 2 * i) / radarData.axes.length - Math.PI / 2}
								{@const lx = 150 + 140 * Math.cos(angle)}
								{@const ly = 150 + 140 * Math.sin(angle)}
								<text x={lx} y={ly} text-anchor="middle" dominant-baseline="middle" class="fill-ink-400 text-[9px]">{axis.name}</text>
							{/each}
						</svg>
					</div>
				</div>
			{/if}

			<!-- Score List -->
			{#if bookScores.length > 0}
				<div class="rounded-xl border border-ink-700/50 bg-ink-900/30 p-4">
					<h3 class="text-sm font-medium text-ink-300 mb-3">浓度排名</h3>
					<div class="space-y-2">
						{#each bookScores as score}
							<div class="flex items-center gap-3">
								<div class="w-2.5 h-2.5 rounded-full shrink-0" style="background-color: {score.color}"></div>
								<span class="text-sm text-ink-200 min-w-[80px]">{score.name}</span>
								<div class="flex-1 h-2 rounded-full bg-ink-800/50 overflow-hidden">
									<div
										class="h-full rounded-full transition-all duration-500"
										style="width: {Math.min(100, score.concentration * 100)}%; background-color: {score.color}"
									></div>
								</div>
								<span class="text-xs text-ink-500 tabular-nums w-12 text-right">{concentrationPercent(score.concentration)}</span>
								{#if score.is_warning}
									<ShieldAlert size={12} class="text-red-400" />
								{/if}
							</div>
						{/each}
					</div>
				</div>
			{/if}
		</div>
	{/if}

	<!-- Tab: Vibe Search -->
	{#if activeTab === 'vibe'}
		<div class="space-y-4">
			<div class="rounded-xl border border-ink-700/50 bg-ink-900/30 p-6 space-y-3">
				<h3 class="text-sm font-medium text-ink-300">氛围搜索</h3>
				<p class="text-xs text-ink-600">粘贴一段文字，系统将在你的整个书库中搜索语义相似的段落</p>
				<textarea
					bind:value={vibeText}
					rows="4"
					placeholder="粘贴一段你喜欢的描写、对话、或情节..."
					class="w-full rounded-lg border border-ink-700/50 bg-ink-800/50 px-4 py-3 text-sm text-ink-200 placeholder-ink-600 focus:border-accent-500/50 focus:outline-none resize-none"
				></textarea>
				<div class="flex flex-col gap-2 sm:flex-row">
					<input
						type="text"
						bind:value={vibeBookmarkName}
						placeholder="保存名称"
						aria-label="氛围书签名称"
						class="min-w-0 flex-1 rounded-lg border border-ink-700/50 bg-ink-800/50 px-3 py-2 text-sm text-ink-200 placeholder-ink-600 focus:border-accent-500/50 focus:outline-none"
					/>
					<button
						onclick={saveCurrentVibe}
						disabled={vibeSaving || !vibeText.trim()}
						aria-label="保存当前氛围检索"
						class="flex items-center justify-center gap-2 rounded-lg border border-ink-700/60 px-4 py-2 text-sm font-medium text-ink-300 transition-colors hover:border-accent-500/40 hover:text-accent-300 disabled:cursor-not-allowed disabled:opacity-50"
					>
						<BookmarkIcon size={14} />
						{vibeSaving ? '保存中...' : '保存氛围'}
					</button>
					<button
						onclick={vibeSearch}
						disabled={vibeSearching || !vibeText.trim()}
						class="flex items-center justify-center gap-2 rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-white hover:bg-accent-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
					>
						{#if vibeSearching}
							搜索中...
						{:else}
							<Search size={14} /> 搜索相似段落
						{/if}
					</button>
				</div>
			</div>

			{#if vibeBookmarks.length > 0}
				<section class="space-y-2" aria-label="已保存氛围">
					<div class="flex items-center justify-between">
						<h3 class="text-sm text-ink-400">已保存氛围</h3>
						<span class="text-xs text-ink-600">{vibeBookmarks.length} 个</span>
					</div>
					<div class="grid gap-2 sm:grid-cols-2">
						{#each vibeBookmarks as bookmark}
							<button
								onclick={() => runVibeBookmark(bookmark)}
								class="min-w-0 rounded-lg border border-ink-800/50 bg-ink-900/30 p-3 text-left transition-colors hover:border-accent-500/30 hover:bg-ink-900/60 focus:outline-none focus:ring-2 focus:ring-accent-500/40"
							>
								<span class="block truncate text-sm font-medium text-ink-200">{bookmark.name ?? '未命名氛围'}</span>
								<span class="mt-1 line-clamp-2 text-xs leading-5 text-ink-500">{bookmark.source_text}</span>
							</button>
						{/each}
					</div>
				</section>
			{/if}

			<!-- Results -->
			{#if vibeResults.length > 0}
				<div class="space-y-3">
					<h3 class="text-sm text-ink-400">{vibeResults.length} 个相似段落</h3>
					{#each vibeResults as result}
						<a href="/reading/{result.book_id}?chapter={result.chapter_index ?? 0}" class="block rounded-xl border border-ink-700/50 bg-ink-900/30 p-4 transition-colors hover:border-accent-500/30 hover:bg-ink-900/60 focus:outline-none focus:ring-2 focus:ring-accent-500/40">
							<div class="flex items-center gap-2 mb-2">
								<BookOpen size={12} class="text-ink-500" />
								<span class="text-xs text-ink-400">
									{result.book_title} · 第{(result.chapter_index ?? 0) + 1}章
								</span>
								<span class="ml-auto text-xs font-mono text-accent-400">
									{((result.similarity ?? result.score) * 100).toFixed(1)}%
								</span>
							</div>
							<p class="text-sm text-ink-300 line-clamp-4 whitespace-pre-wrap">{result.content ?? result.snippet}</p>
						</a>
					{/each}
				</div>
			{/if}
		</div>
	{/if}
</div>
