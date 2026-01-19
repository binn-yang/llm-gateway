import { type ChartOptions, type ChartData, type ChartType } from 'chart.js'

export interface ChartConfig {
  type: ChartType
  data: ChartData
  options?: ChartOptions
}

/**
 * Create gradient for chart area fill
 */
export function createGradient(
  ctx: CanvasRenderingContext2D,
  colorStart: string,
  colorEnd: string
): CanvasGradient {
  const gradient = ctx.createLinearGradient(0, 0, 0, 400)
  gradient.addColorStop(0, colorStart)
  gradient.addColorStop(1, colorEnd)
  return gradient
}

/**
 * Default chart options for line charts
 */
export function getDefaultLineOptions(): ChartOptions<'line'> {
  return {
    responsive: true,
    maintainAspectRatio: false,
    plugins: {
      legend: {
        display: true,
        labels: {
          color: 'rgba(255, 255, 255, 0.8)',
          font: {
            size: 12,
          },
        },
      },
      tooltip: {
        mode: 'index',
        intersect: false,
        backgroundColor: 'rgba(0, 0, 0, 0.8)',
        titleColor: '#fff',
        bodyColor: '#fff',
        borderColor: 'rgba(255, 255, 255, 0.2)',
        borderWidth: 1,
      },
    },
    scales: {
      x: {
        grid: {
          color: 'rgba(255, 255, 255, 0.1)',
        },
        ticks: {
          color: 'rgba(255, 255, 255, 0.6)',
        },
      },
      y: {
        grid: {
          color: 'rgba(255, 255, 255, 0.1)',
        },
        ticks: {
          color: 'rgba(255, 255, 255, 0.6)',
        },
      },
    },
    interaction: {
      mode: 'nearest',
      axis: 'x',
      intersect: false,
    },
  }
}

/**
 * Default chart options for bar charts
 */
export function getDefaultBarOptions(): ChartOptions<'bar'> {
  return {
    responsive: true,
    maintainAspectRatio: false,
    plugins: {
      legend: {
        display: false,
      },
      tooltip: {
        backgroundColor: 'rgba(0, 0, 0, 0.8)',
        titleColor: '#fff',
        bodyColor: '#fff',
        borderColor: 'rgba(255, 255, 255, 0.2)',
        borderWidth: 1,
      },
    },
    scales: {
      x: {
        grid: {
          display: false,
        },
        ticks: {
          color: 'rgba(255, 255, 255, 0.6)',
        },
      },
      y: {
        grid: {
          color: 'rgba(255, 255, 255, 0.1)',
        },
        ticks: {
          color: 'rgba(255, 255, 255, 0.6)',
        },
      },
    },
  }
}

/**
 * Color schemes for charts
 */
export const chartColors = {
  primary: 'rgba(102, 126, 234, 1)', // #667eea
  secondary: 'rgba(118, 75, 162, 1)', // #764ba2
  accent: 'rgba(240, 147, 251, 1)', // #f093fb
  success: 'rgba(16, 185, 129, 1)', // #10b981
  warning: 'rgba(245, 158, 11, 1)', // #f59e0b
  danger: 'rgba(239, 68, 68, 1)', // #ef4444

  // With opacity for fills
  primaryFill: 'rgba(102, 126, 234, 0.5)',
  secondaryFill: 'rgba(118, 75, 162, 0.5)',
  accentFill: 'rgba(240, 147, 251, 0.5)',
  successFill: 'rgba(16, 185, 129, 0.5)',
  warningFill: 'rgba(245, 158, 11, 0.5)',
  dangerFill: 'rgba(239, 68, 68, 0.5)',
}
