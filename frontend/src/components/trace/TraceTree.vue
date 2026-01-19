<template>
  <el-card class="tree-card glass-strong" shadow="hover">
    <template #header>
      <span class="text-white font-bold">Span Tree</span>
    </template>

    <el-table
      :data="treeData"
      row-key="span_id"
      :tree-props="{ children: 'children', hasChildren: 'hasChildren' }"
      stripe
      default-expand-all
      :header-cell-style="{ background: 'rgba(255,255,255,0.1)', color: '#fff' }"
      :cell-style="{ color: 'rgba(255,255,255,0.8)' }"
    >
      <el-table-column prop="name" label="Span Name" min-width="300" />

      <el-table-column prop="kind" label="Type" width="100">
        <template #default="{ row }">
          <el-tag :type="getKindType(row.kind)" size="small">
            {{ row.kind }}
          </el-tag>
        </template>
      </el-table-column>

      <el-table-column prop="duration_ms" label="Duration" width="120" align="right">
        <template #default="{ row }">
          {{ formatDuration(row.duration_ms) }}
        </template>
      </el-table-column>

      <el-table-column prop="status" label="Status" width="100">
        <template #default="{ row }">
          <el-tag :type="row.status === 'ok' ? 'success' : 'danger'" size="small">
            {{ row.status }}
          </el-tag>
        </template>
      </el-table-column>
    </el-table>
  </el-card>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { Span } from '@/api/traces'

const props = defineProps<{
  spans: Span[]
}>()

// Build tree structure from flat spans
interface TreeNode extends Span {
  children?: TreeNode[]
  hasChildren?: boolean
}

const treeData = computed<TreeNode[]>(() => {
  if (!props.spans || props.spans.length === 0) return []

  const spanMap = new Map<string, TreeNode>()
  const rootSpans: TreeNode[] = []

  // First pass: create nodes
  props.spans.forEach((span) => {
    const node: TreeNode = { ...span }
    spanMap.set(span.span_id, node)
  })

  // Second pass: build tree
  props.spans.forEach((span) => {
    const node = spanMap.get(span.span_id)!

    if (span.parent_id) {
      const parent = spanMap.get(span.parent_id)
      if (parent) {
        if (!parent.children) {
          parent.children = []
        }
        parent.children.push(node)
        parent.hasChildren = true
      }
    } else {
      rootSpans.push(node)
    }
  })

  return rootSpans
})

function formatDuration(ms: number): string {
  if (ms < 1) return `${(ms * 1000).toFixed(0)}μs`
  if (ms < 1000) return `${ms.toFixed(2)}ms`
  return `${(ms / 1000).toFixed(2)}s`
}

function getKindType(kind: string): 'primary' | 'success' | 'warning' | 'info' {
  switch (kind.toLowerCase()) {
    case 'server':
      return 'primary'
    case 'client':
      return 'success'
    case 'internal':
      return 'info'
    default:
      return 'info'
  }
}
</script>
