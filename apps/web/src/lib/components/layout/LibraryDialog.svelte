<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { api } from '$services/api';
	import { getErrorMessage } from '$lib/utils';
	import { Database, Fingerprint, FolderCog, Gauge, ImageOff, LockKeyhole, RotateCcw, Settings2, SlidersHorizontal, UsersRound, Wand2, Wrench } from 'lucide-svelte';
	import type { PermissionTemplate } from '$types/models';

	type LibraryPanel = 'basic' | 'scan' | 'permissions' | 'features' | 'maintenance';
	type LibraryDialogValue = {
		id: string;
		name: string;
		root_path: string;
		description?: string | null;
		auto_scan?: boolean;
		scan_interval_secs?: number;
		include_extensions?: string[];
		exclude_patterns?: string[];
		book_count?: number;
		last_scan_at?: string | null;
		scan_status?: string;
	};

	interface Props {
		open: boolean;
		mode: 'create' | 'edit';
		library?: LibraryDialogValue;
		onclose: () => void;
		onsaved: (library: LibraryDialogValue) => void;
	}

	let { open = $bindable(), mode, library, onclose, onsaved }: Props = $props();

	let activePanel = $state<LibraryPanel>('basic');
	let name = $state('');
	let rootPath = $state('');
	let description = $state('');
	let autoScan = $state(true);
	let scanIntervalHours = $state(1);
	let includeExtensions = $state('txt, epub, pdf, docx, doc, md, html');
	let excludePatterns = $state('.*, ~$*, *.tmp');
	let enableAi = $state(true);
	let enableTranslation = $state(true);
	let enableGraph = $state(true);
	let allowGuests = $state(false);
	let saving = $state(false);
	let error = $state('');

	type PermUser = { id: string; username: string; display_name: string | null };
	type PermGroup = { id: string; name: string; description: string; member_count: number };
	type Perm = { can_read: boolean; can_write: boolean; can_manage: boolean };
	type MaintenanceAction = 'reindex' | 'cleanup-orphan-covers' | 'recompute-hashes';
	let permissionUsers = $state<PermUser[]>([]);
	let permissionGroups = $state<PermGroup[]>([]);
	let permissionMap = $state<Record<string, Perm>>({});
	let groupPermissionMap = $state<Record<string, Perm>>({});
	let permissionsLoaded = $state(false);
	let permissionTemplates = $state<PermissionTemplate[]>([]);
	let maintenanceRunning = $state<Partial<Record<MaintenanceAction, boolean>>>({});
	let maintenanceMessage = $state('');

	const fallbackPermissionTemplates: PermissionTemplate[] = [
		{ id: 'builtin-read', name: '只读', description: '可浏览和阅读书库内容', can_read: true, can_write: false, can_manage: false, is_system: true },
		{ id: 'builtin-write', name: '协作者', description: '可阅读并编辑书库内容', can_read: true, can_write: true, can_manage: false, is_system: true },
		{ id: 'builtin-manage', name: '管理员', description: '可管理书库设置和权限', can_read: true, can_write: true, can_manage: true, is_system: true },
	];

	const panels: Array<{ id: LibraryPanel; label: string; description: string; icon: typeof FolderCog }> = [
		{ id: 'basic', label: '基础信息', description: '名称、目录、描述', icon: FolderCog },
		{ id: 'scan', label: '扫描规则', description: '格式、排除、频率', icon: SlidersHorizontal },
		{ id: 'permissions', label: '权限', description: '访问范围与协作', icon: LockKeyhole },
		{ id: 'features', label: '功能开关', description: 'AI、翻译、图谱', icon: Settings2 },
		{ id: 'maintenance', label: '维护', description: '状态与任务', icon: Wrench },
	];

	$effect(() => {
		if (!open) return;
		activePanel = 'basic';
		maintenanceRunning = {};
		maintenanceMessage = '';
		if (mode === 'edit' && library) {
			name = library.name;
			rootPath = library.root_path;
			description = library.description ?? '';
			autoScan = library.auto_scan ?? true;
			scanIntervalHours = Math.max(1, Math.round((library.scan_interval_secs ?? 3600) / 3600));
			includeExtensions = (library.include_extensions?.length ? library.include_extensions : ['txt', 'epub', 'pdf', 'docx', 'doc', 'md', 'html']).join(', ');
			excludePatterns = (library.exclude_patterns?.length ? library.exclude_patterns : ['.*', '~$*', '*.tmp']).join(', ');
			loadFeatures(library.id);
			loadPermissions(library.id);
		} else if (mode === 'create') {
			name = '';
			rootPath = '';
			description = '';
			autoScan = true;
			scanIntervalHours = 1;
			includeExtensions = 'txt, epub, pdf, docx, doc, md, html';
			excludePatterns = '.*, ~$*, *.tmp';
			enableAi = true;
			enableTranslation = true;
			enableGraph = true;
			allowGuests = false;
			permissionUsers = [];
			permissionGroups = [];
			permissionMap = {};
			groupPermissionMap = {};
			permissionTemplates = [];
			permissionsLoaded = false;
		}
		error = '';
	});

	async function loadFeatures(libraryId: string) {
		try {
			const features = await api.getLibraryFeatures(libraryId);
			enableAi = features.enable_ai;
			enableTranslation = features.enable_translation;
			enableGraph = features.enable_graph;
			allowGuests = features.allow_guests;
		} catch {
			/* keep defaults if the library has no features yet */
		}
	}

	async function loadPermissions(libraryId: string) {
		permissionsLoaded = false;
		try {
			const [users, groups, perms, templates] = await Promise.all([
				api.getUsers(),
				api.getGroups(),
				api.getLibraryPermissions(libraryId),
				api.getPermissionTemplates().catch(() => fallbackPermissionTemplates),
			]);
			permissionUsers = users.map((u) => ({ id: u.id, username: u.username, display_name: u.display_name }));
			permissionGroups = groups.map((g) => ({ id: g.id, name: g.name, description: g.description, member_count: g.member_count }));
			permissionTemplates = templates.length > 0 ? templates : fallbackPermissionTemplates;
			const map: Record<string, Perm> = {};
			for (const u of users) {
				const existing = perms.permissions.find((p) => p.user_id === u.id);
				map[u.id] = existing
					? { can_read: existing.can_read, can_write: existing.can_write, can_manage: existing.can_manage }
					: { can_read: false, can_write: false, can_manage: false };
			}
			permissionMap = map;

			const groupMap: Record<string, Perm> = {};
			for (const g of groups) {
				const existing = perms.group_permissions.find((p) => p.group_id === g.id);
				groupMap[g.id] = existing
					? { can_read: existing.can_read, can_write: existing.can_write, can_manage: existing.can_manage }
					: { can_read: false, can_write: false, can_manage: false };
			}
			groupPermissionMap = groupMap;
			permissionsLoaded = true;
		} catch {
			permissionUsers = [];
			permissionGroups = [];
			permissionMap = {};
			groupPermissionMap = {};
			permissionTemplates = fallbackPermissionTemplates;
			permissionsLoaded = true;
		}
	}

	function nextPermission(base: Perm | undefined, key: keyof Perm, checked: boolean): Perm {
		const next: Perm = { can_read: false, can_write: false, can_manage: false, ...(base ?? {}) };
		next[key] = checked;
		if (key === 'can_manage' && checked) {
			next.can_read = true;
			next.can_write = true;
		}
		if (key === 'can_write' && checked) next.can_read = true;
		if (key === 'can_read' && !checked) {
			next.can_write = false;
			next.can_manage = false;
		}
		if (key === 'can_write' && !checked) next.can_manage = false;
		return next;
	}

	function setUserPermission(userId: string, key: keyof Perm, checked: boolean) {
		permissionMap = {
			...permissionMap,
			[userId]: nextPermission(permissionMap[userId], key, checked),
		};
	}

	function setGroupPermission(groupId: string, key: keyof Perm, checked: boolean) {
		groupPermissionMap = {
			...groupPermissionMap,
			[groupId]: nextPermission(groupPermissionMap[groupId], key, checked),
		};
	}

	function permissionFromTemplate(templateId: string): Perm | null {
		const template = permissionTemplates.find((item) => item.id === templateId);
		if (!template) return null;
		return {
			can_read: template.can_read,
			can_write: template.can_write,
			can_manage: template.can_manage,
		};
	}

	function applyUserTemplate(userId: string, templateId: string) {
		const next = permissionFromTemplate(templateId);
		if (!next) return;
		permissionMap = { ...permissionMap, [userId]: next };
	}

	function applyGroupTemplate(groupId: string, templateId: string) {
		const next = permissionFromTemplate(templateId);
		if (!next) return;
		groupPermissionMap = { ...groupPermissionMap, [groupId]: next };
	}

	function splitList(value: string): string[] {
		return value
			.split(/[\n,]/)
			.map((item) => item.trim().replace(/^\./, ''))
			.filter(Boolean);
	}

	async function runMaintenance(action: MaintenanceAction) {
		if (!library?.id || maintenanceRunning[action]) return;
		maintenanceRunning = { ...maintenanceRunning, [action]: true };
		maintenanceMessage = '';
		error = '';
		try {
			const result = await api.runLibraryMaintenance(library.id, action);
			maintenanceMessage = result.message || '维护任务已加入队列';
		} catch (err: unknown) {
			error = getErrorMessage(err) ?? '维护任务提交失败';
		} finally {
			maintenanceRunning = { ...maintenanceRunning, [action]: false };
		}
	}

	async function handleSubmit(e: SubmitEvent) {
		e.preventDefault();
		if (!name.trim() || !rootPath.trim()) {
			error = '名称和路径不能为空';
			activePanel = 'basic';
			return;
		}

		saving = true;
		error = '';

		try {
			const payload = {
				name: name.trim(),
				root_path: rootPath.trim(),
				description: description.trim() || undefined,
				auto_scan: autoScan,
				scan_interval_secs: Math.max(1, scanIntervalHours) * 3600,
				include_extensions: splitList(includeExtensions),
				exclude_patterns: splitList(excludePatterns),
			};

			const result = mode === 'create'
				? await api.createLibrary(payload)
				: library
					? await api.updateLibrary(library.id, payload)
					: undefined;

			// Persist library-level feature toggles for the saved library.
			if (result?.id) {
				try {
					await api.setLibraryFeatures(result.id, {
						enable_ai: enableAi,
						enable_translation: enableTranslation,
						enable_graph: enableGraph,
						allow_guests: allowGuests,
					});
				} catch {
					/* feature persistence is best-effort; don't block the save */
				}

				// Persist per-user library permissions (only when the editor was loaded).
				if (permissionsLoaded && (permissionUsers.length > 0 || permissionGroups.length > 0)) {
					try {
						await api.setLibraryPermissions(result.id, {
							permissions: permissionUsers.map((u) => ({
								user_id: u.id,
								can_read: permissionMap[u.id]?.can_read ?? false,
								can_write: permissionMap[u.id]?.can_write ?? false,
								can_manage: permissionMap[u.id]?.can_manage ?? false,
							})),
							group_permissions: permissionGroups.map((g) => ({
								group_id: g.id,
								can_read: groupPermissionMap[g.id]?.can_read ?? false,
								can_write: groupPermissionMap[g.id]?.can_write ?? false,
								can_manage: groupPermissionMap[g.id]?.can_manage ?? false,
							})),
						});
					} catch {
						/* permission persistence is best-effort; don't block the save */
					}
				}
			}

			onsaved(result!);
			open = false;
		} catch (err: unknown) {
			error = getErrorMessage(err) ?? '操作失败';
		} finally {
			saving = false;
		}
	}
</script>

<Dialog.Root bind:open onOpenChange={(v) => { if (!v) onclose(); }}>
	<Dialog.Content class="max-h-[88vh] overflow-hidden border-ink-800/60 bg-ink-900 p-0 text-ink-100 sm:max-w-4xl">
		<form onsubmit={handleSubmit} class="grid max-h-[88vh] grid-cols-1 overflow-hidden md:grid-cols-[230px_minmax(0,1fr)]">
			<aside class="border-b border-ink-800/60 bg-ink-950/70 p-3 md:border-b-0 md:border-r">
				<Dialog.Header class="px-2 py-2 text-left">
					<Dialog.Title class="text-ink-50">{mode === 'create' ? '新建书库' : '书库管理'}</Dialog.Title>
					<Dialog.Description class="text-ink-500">
						{mode === 'create' ? '添加目录并配置默认扫描规则' : '管理扫描、权限和书库级功能'}
					</Dialog.Description>
				</Dialog.Header>

				<div class="mt-3 space-y-1">
					{#each panels as panel}
						{@const Icon = panel.icon}
						<button type="button" onclick={() => activePanel = panel.id} class="flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left transition-colors {activePanel === panel.id ? 'bg-accent-500/10 text-accent-300' : 'text-ink-400 hover:bg-ink-800/60 hover:text-ink-100'}">
							<Icon size={16} />
							<span class="min-w-0">
								<span class="block text-sm font-medium">{panel.label}</span>
								<span class="block truncate text-[11px] opacity-70">{panel.description}</span>
							</span>
						</button>
					{/each}
				</div>
			</aside>

			<section class="flex min-h-0 flex-col">
				<div class="min-h-0 flex-1 overflow-y-auto p-5">
					{#if activePanel === 'basic'}
						<div class="space-y-5">
							<div>
								<h3 class="text-base font-semibold text-ink-100">基础信息</h3>
								<p class="mt-1 text-sm text-ink-500">书库名称会出现在侧栏和筛选器中，根目录用于扫描文件。</p>
							</div>
							<div class="grid gap-4 md:grid-cols-2">
								<div class="space-y-2">
									<label for="lib-name" class="text-sm font-medium text-ink-300">名称</label>
									<Input id="lib-name" bind:value={name} placeholder="我的书库" class="border-ink-700/60 bg-ink-800/50 text-ink-100 placeholder:text-ink-600" />
								</div>
								<div class="space-y-2">
									<label for="lib-path" class="text-sm font-medium text-ink-300">根目录路径</label>
									<Input id="lib-path" bind:value={rootPath} placeholder="/path/to/books" class="border-ink-700/60 bg-ink-800/50 font-mono text-sm text-ink-100 placeholder:text-ink-600" />
								</div>
							</div>
							<div class="space-y-2">
								<label for="lib-desc" class="text-sm font-medium text-ink-300">描述</label>
								<Input id="lib-desc" bind:value={description} placeholder="小说、资料、技术书..." class="border-ink-700/60 bg-ink-800/50 text-ink-100 placeholder:text-ink-600" />
							</div>
						</div>
					{:else if activePanel === 'scan'}
						<div class="space-y-5">
							<div>
								<h3 class="text-base font-semibold text-ink-100">扫描规则</h3>
								<p class="mt-1 text-sm text-ink-500">配置支持格式、排除规则和自动扫描频率。</p>
							</div>
							<label class="flex items-center justify-between rounded-lg border border-ink-800/60 bg-ink-950/30 p-3">
								<span>
									<span class="block text-sm text-ink-200">自动扫描</span>
									<span class="text-xs text-ink-500">开启后按周期检测新增和修改文件</span>
								</span>
								<input type="checkbox" bind:checked={autoScan} class="h-4 w-4 rounded border-ink-600 bg-ink-800 text-accent-500" />
							</label>
							<div class="grid gap-4 md:grid-cols-2">
								<div class="space-y-2">
									<label for="scan-interval" class="text-sm font-medium text-ink-300">扫描间隔（小时）</label>
									<Input id="scan-interval" type="number" min="1" bind:value={scanIntervalHours} class="border-ink-700/60 bg-ink-800/50 text-ink-100" />
								</div>
								<div class="space-y-2">
									<label for="include-exts" class="text-sm font-medium text-ink-300">支持格式</label>
									<Input id="include-exts" bind:value={includeExtensions} class="border-ink-700/60 bg-ink-800/50 font-mono text-sm text-ink-100" />
								</div>
							</div>
							<div class="space-y-2">
								<label for="exclude-patterns" class="text-sm font-medium text-ink-300">排除规则</label>
								<Input id="exclude-patterns" bind:value={excludePatterns} class="border-ink-700/60 bg-ink-800/50 font-mono text-sm text-ink-100" />
							</div>
						</div>
					{:else if activePanel === 'permissions'}
						<div class="space-y-5">
							<div>
								<h3 class="text-base font-semibold text-ink-100">权限</h3>
								<p class="mt-1 text-sm text-ink-500">按用户组或单个用户授予书库访问能力。</p>
							</div>
							<label class="flex items-center justify-between rounded-lg border border-ink-800/60 bg-ink-950/30 p-3">
								<span>
									<span class="block text-sm text-ink-200">允许访客浏览</span>
									<span class="text-xs text-ink-500">访客角色可看到此书库</span>
								</span>
								<input type="checkbox" bind:checked={allowGuests} class="h-4 w-4" />
							</label>
							{#if permissionTemplates.length > 0}
								<div class="flex flex-wrap items-center gap-2 rounded-lg border border-ink-800/60 bg-ink-950/30 p-3 text-xs text-ink-400">
									<Wand2 class="h-4 w-4 text-accent-400" />
									{#each permissionTemplates as template}
										<span class="rounded-md border border-ink-800/70 px-2 py-1 text-ink-300">{template.name}</span>
									{/each}
								</div>
							{/if}

							{#if !permissionsLoaded}
								<div class="space-y-2">
									<div class="h-16 rounded-lg bg-ink-950/50 animate-pulse"></div>
									<div class="h-16 rounded-lg bg-ink-950/50 animate-pulse"></div>
								</div>
							{:else}
								<div class="space-y-3">
									<div class="flex items-center gap-2">
										<UsersRound class="h-4 w-4 text-accent-400" />
										<h4 class="text-sm font-medium text-ink-200">用户组授权</h4>
									</div>
									{#if permissionGroups.length > 0}
										<div class="overflow-hidden rounded-lg border border-ink-800/60">
											<table class="w-full text-sm">
												<thead class="bg-ink-950/60 text-xs text-ink-500">
													<tr>
														<th class="px-3 py-2 text-left">用户组</th>
														<th class="w-32 px-3 py-2 text-left">模板</th>
														<th class="w-20 px-3 py-2 text-center">阅读</th>
														<th class="w-20 px-3 py-2 text-center">编辑</th>
														<th class="w-20 px-3 py-2 text-center">管理</th>
													</tr>
												</thead>
												<tbody class="divide-y divide-ink-800/40">
													{#each permissionGroups as group}
														{@const perm = groupPermissionMap[group.id] ?? { can_read: false, can_write: false, can_manage: false }}
														<tr class="bg-ink-950/20">
															<td class="px-3 py-2">
																<div class="font-medium text-ink-200">{group.name}</div>
																<div class="text-xs text-ink-500">{group.member_count} 位成员{group.description ? ` · ${group.description}` : ''}</div>
															</td>
															<td class="px-3 py-2">
																<select aria-label="{group.name} 权限模板" class="w-full rounded-md border border-ink-700/60 bg-ink-900 px-2 py-1 text-xs text-ink-200" onchange={(e) => { applyGroupTemplate(group.id, e.currentTarget.value); e.currentTarget.value = ''; }}>
																	<option value="">套用</option>
																	{#each permissionTemplates as template}
																		<option value={template.id}>{template.name}</option>
																	{/each}
																</select>
															</td>
															<td class="px-3 py-2 text-center">
																<input aria-label="{group.name} 阅读权限" type="checkbox" checked={perm.can_read} onchange={(e) => setGroupPermission(group.id, 'can_read', e.currentTarget.checked)} class="h-4 w-4 rounded border-ink-600 bg-ink-800 text-accent-500" />
															</td>
															<td class="px-3 py-2 text-center">
																<input aria-label="{group.name} 编辑权限" type="checkbox" checked={perm.can_write} onchange={(e) => setGroupPermission(group.id, 'can_write', e.currentTarget.checked)} class="h-4 w-4 rounded border-ink-600 bg-ink-800 text-accent-500" />
															</td>
															<td class="px-3 py-2 text-center">
																<input aria-label="{group.name} 管理权限" type="checkbox" checked={perm.can_manage} onchange={(e) => setGroupPermission(group.id, 'can_manage', e.currentTarget.checked)} class="h-4 w-4 rounded border-ink-600 bg-ink-800 text-accent-500" />
															</td>
														</tr>
													{/each}
												</tbody>
											</table>
										</div>
									{:else}
										<div class="rounded-lg border border-dashed border-ink-800/70 px-4 py-5 text-sm text-ink-500">暂无用户组。</div>
									{/if}
								</div>

								<div class="space-y-3">
									<h4 class="text-sm font-medium text-ink-200">单个用户授权</h4>
									{#if permissionUsers.length > 0}
										<div class="max-h-72 overflow-y-auto rounded-lg border border-ink-800/60">
											<table class="w-full text-sm">
												<thead class="sticky top-0 bg-ink-950 text-xs text-ink-500">
													<tr>
														<th class="px-3 py-2 text-left">用户</th>
														<th class="w-32 px-3 py-2 text-left">模板</th>
														<th class="w-20 px-3 py-2 text-center">阅读</th>
														<th class="w-20 px-3 py-2 text-center">编辑</th>
														<th class="w-20 px-3 py-2 text-center">管理</th>
													</tr>
												</thead>
												<tbody class="divide-y divide-ink-800/40">
													{#each permissionUsers as user}
														{@const perm = permissionMap[user.id] ?? { can_read: false, can_write: false, can_manage: false }}
														<tr class="bg-ink-950/20">
															<td class="px-3 py-2">
																<div class="font-medium text-ink-200">{user.display_name ?? user.username}</div>
																<div class="text-xs text-ink-500">@{user.username}</div>
															</td>
															<td class="px-3 py-2">
																<select aria-label="{user.username} 权限模板" class="w-full rounded-md border border-ink-700/60 bg-ink-900 px-2 py-1 text-xs text-ink-200" onchange={(e) => { applyUserTemplate(user.id, e.currentTarget.value); e.currentTarget.value = ''; }}>
																	<option value="">套用</option>
																	{#each permissionTemplates as template}
																		<option value={template.id}>{template.name}</option>
																	{/each}
																</select>
															</td>
															<td class="px-3 py-2 text-center">
																<input aria-label="{user.username} 阅读权限" type="checkbox" checked={perm.can_read} onchange={(e) => setUserPermission(user.id, 'can_read', e.currentTarget.checked)} class="h-4 w-4 rounded border-ink-600 bg-ink-800 text-accent-500" />
															</td>
															<td class="px-3 py-2 text-center">
																<input aria-label="{user.username} 编辑权限" type="checkbox" checked={perm.can_write} onchange={(e) => setUserPermission(user.id, 'can_write', e.currentTarget.checked)} class="h-4 w-4 rounded border-ink-600 bg-ink-800 text-accent-500" />
															</td>
															<td class="px-3 py-2 text-center">
																<input aria-label="{user.username} 管理权限" type="checkbox" checked={perm.can_manage} onchange={(e) => setUserPermission(user.id, 'can_manage', e.currentTarget.checked)} class="h-4 w-4 rounded border-ink-600 bg-ink-800 text-accent-500" />
															</td>
														</tr>
													{/each}
												</tbody>
											</table>
										</div>
									{:else}
										<div class="rounded-lg border border-dashed border-ink-800/70 px-4 py-5 text-sm text-ink-500">暂无可授权用户。</div>
									{/if}
								</div>
							{/if}
						</div>
					{:else if activePanel === 'features'}
						<div class="space-y-4">
							<h3 class="text-base font-semibold text-ink-100">书库级功能开关</h3>
							{#each [
								{ label: 'AI 分析', value: enableAi, setter: (value: boolean) => enableAi = value, description: '摘要、标签、人物、深度分析' },
								{ label: '沉浸式翻译', value: enableTranslation, setter: (value: boolean) => enableTranslation = value, description: '阅读器双语和仅译文模式' },
								{ label: '知识图谱', value: enableGraph, setter: (value: boolean) => enableGraph = value, description: '实体抽取与关系图谱' },
							] as item}
								<label class="flex items-center justify-between rounded-lg border border-ink-800/60 bg-ink-950/30 p-3">
									<span>
										<span class="block text-sm text-ink-200">{item.label}</span>
										<span class="text-xs text-ink-500">{item.description}</span>
									</span>
									<input type="checkbox" checked={item.value} onchange={(e) => item.setter(e.currentTarget.checked)} class="h-4 w-4" />
								</label>
							{/each}
						</div>
					{:else if activePanel === 'maintenance'}
						<div class="space-y-5">
							<h3 class="text-base font-semibold text-ink-100">维护</h3>
							<div class="grid gap-3 sm:grid-cols-3">
								<div class="rounded-lg border border-ink-800/60 bg-ink-950/30 p-3"><Database class="mb-2 h-4 w-4 text-ink-500" /><p class="text-xs text-ink-500">书籍</p><p class="mt-1 text-lg font-semibold text-ink-100">{library?.book_count ?? 0}</p></div>
								<div class="rounded-lg border border-ink-800/60 bg-ink-950/30 p-3"><Gauge class="mb-2 h-4 w-4 text-ink-500" /><p class="text-xs text-ink-500">扫描状态</p><p class="mt-1 text-sm font-semibold text-ink-100">{library?.scan_status ?? 'idle'}</p></div>
								<div class="rounded-lg border border-ink-800/60 bg-ink-950/30 p-3"><Wrench class="mb-2 h-4 w-4 text-ink-500" /><p class="text-xs text-ink-500">上次扫描</p><p class="mt-1 text-sm font-semibold text-ink-100">{library?.last_scan_at ? new Date(library.last_scan_at).toLocaleDateString() : '尚未扫描'}</p></div>
							</div>
							<div class="grid gap-3">
								{#each [
									{ action: 'reindex' as MaintenanceAction, label: '重建索引', description: '重新生成向量与全文索引任务', icon: RotateCcw },
									{ action: 'cleanup-orphan-covers' as MaintenanceAction, label: '清理孤儿封面', description: '删除未被书籍或系列引用的封面文件', icon: ImageOff },
									{ action: 'recompute-hashes' as MaintenanceAction, label: '重新计算哈希', description: '按当前文件重新写入 SHA-256 指纹', icon: Fingerprint },
								] as item}
									{@const Icon = item.icon}
									<div class="flex items-center justify-between gap-3 rounded-lg border border-ink-800/60 bg-ink-950/30 p-3">
										<div class="flex min-w-0 items-center gap-3">
											<div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-ink-800/70 text-ink-300">
												<Icon class="h-4 w-4" />
											</div>
											<span class="min-w-0">
												<span class="block text-sm font-medium text-ink-200">{item.label}</span>
												<span class="block truncate text-xs text-ink-500">{item.description}</span>
											</span>
										</div>
										<Button type="button" variant="outline" disabled={mode === 'create' || maintenanceRunning[item.action]} onclick={() => runMaintenance(item.action)} class="shrink-0 border-ink-700/60 text-ink-300 hover:bg-ink-800/50 hover:text-ink-100">
											{maintenanceRunning[item.action] ? '排队中...' : '加入队列'}
										</Button>
									</div>
								{/each}
							</div>
							{#if maintenanceMessage}
								<p class="rounded-lg border border-emerald-500/20 bg-emerald-500/10 px-3 py-2 text-sm text-emerald-300">{maintenanceMessage}</p>
							{/if}
						</div>
					{/if}

					{#if error}
						<p class="mt-5 rounded-lg border border-red-500/20 bg-red-500/10 px-3 py-2 text-sm text-red-300">{error}</p>
					{/if}
				</div>

				<Dialog.Footer class="border-t border-ink-800/60 bg-ink-950/50 p-4">
					<Button variant="outline" type="button" onclick={() => { open = false; onclose(); }} class="border-ink-700/60 text-ink-300 hover:bg-ink-800/50 hover:text-ink-100">取消</Button>
					<Button type="submit" disabled={saving} class="bg-accent-500 text-ink-950 hover:bg-accent-600">{saving ? '保存中...' : mode === 'create' ? '创建书库' : '保存书库'}</Button>
				</Dialog.Footer>
			</section>
		</form>
	</Dialog.Content>
</Dialog.Root>
