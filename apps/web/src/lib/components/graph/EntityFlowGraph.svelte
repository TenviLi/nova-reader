<script lang="ts">
	import {
		SvelteFlow,
		Controls,
		Background,
		MiniMap,
		type Node,
		type Edge,
		type NodeTypes,
		Position,
	} from '@xyflow/svelte';
	import '@xyflow/svelte/dist/style.css';
	import EntityNode from './EntityNode.svelte';

	let { nodes: inputNodes, edges: inputEdges, onNodeClick } = $props<{
		nodes: Array<{ id: string; label: string; type: string; size: number }>;
		edges: Array<{ source: string; target: string; label: string; weight?: number }>;
		onNodeClick?: (nodeId: string) => void;
	}>();

	const typeColors: Record<string, string> = {
		person: '#f59e0b',
		location: '#22c55e',
		organization: '#6366f1',
		item: '#ec4899',
		concept: '#8b5cf6',
		event: '#ef4444',
	};

	const nodeTypes: NodeTypes = {
		entity: EntityNode,
	};

	// Convert input data to Svelte Flow format with auto-layout
	let flowNodes = $derived<Node[]>(layoutNodes(inputNodes));
	let flowEdges = $derived<Edge[]>(inputEdges.map((e: typeof inputEdges[number]) => ({
		id: `${e.source}-${e.target}`,
		source: e.source,
		target: e.target,
		label: e.label,
		animated: false,
		style: `stroke: ${typeColors[getNodeType(e.source)] ?? '#64748b'}; stroke-width: ${Math.max(1, (e.weight ?? 1) * 2)}px`,
		labelStyle: 'font-size: 10px; fill: #94a3b8',
	})));

	function getNodeType(id: string): string {
		return inputNodes.find((n: typeof inputNodes[number]) => n.id === id)?.type ?? 'concept';
	}

	// Simple circular layout with type grouping
	function layoutNodes(nodes: Array<{ id: string; label: string; type: string; size: number }>): Node[] {
		const typeGroups = new Map<string, typeof nodes>();
		for (const node of nodes) {
			const group = typeGroups.get(node.type) ?? [];
			group.push(node);
			typeGroups.set(node.type, group);
		}

		const result: Node[] = [];
		let groupAngle = 0;
		const groupStep = (2 * Math.PI) / Math.max(typeGroups.size, 1);
		const baseRadius = Math.max(200, nodes.length * 15);

		for (const [type, groupNodes] of typeGroups) {
			const groupCenterX = 400 + Math.cos(groupAngle) * baseRadius * 0.6;
			const groupCenterY = 300 + Math.sin(groupAngle) * baseRadius * 0.6;

			const nodeStep = (2 * Math.PI) / Math.max(groupNodes.length, 1);
			const subRadius = Math.max(80, groupNodes.length * 20);

			for (let i = 0; i < groupNodes.length; i++) {
				const node = groupNodes[i];
				const angle = i * nodeStep;
				result.push({
					id: node.id,
					type: 'entity',
					position: {
						x: groupCenterX + Math.cos(angle) * subRadius,
						y: groupCenterY + Math.sin(angle) * subRadius,
					},
					data: {
						label: node.label,
						entityType: node.type,
						size: node.size,
						color: typeColors[node.type] ?? '#64748b',
					},
					sourcePosition: Position.Right,
					targetPosition: Position.Left,
				});
			}
			groupAngle += groupStep;
		}

		return result;
	}

	function handleNodeClick(event: { detail: { node: Node } }) {
		onNodeClick?.(event.detail.node.id);
	}
</script>

<div class="h-full w-full" style="--xy-background-color: transparent; --xy-node-border-radius: 9999px;">
	<SvelteFlow
		nodes={flowNodes}
		edges={flowEdges}
		{nodeTypes}
		fitView
		minZoom={0.1}
		maxZoom={3}
		onnodeclick={(e: any) => handleNodeClick(e)}
		proOptions={{ hideAttribution: true }}
	>
		<Controls position="bottom-right" />
		<Background gap={20} />
		<MiniMap
			position="bottom-left"
			nodeColor={(node: any) => node.data?.color ?? '#64748b'}
			style="background: rgba(15, 23, 42, 0.5); border-radius: 8px; border: 1px solid rgba(51, 65, 85, 0.5);"
		/>
	</SvelteFlow>
</div>
