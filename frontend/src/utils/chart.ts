import {
  Chart,
  CategoryScale,
  LinearScale,
  BarElement,
  LineElement,
  PointElement,
  BarController,
  LineController,
  Title,
  Tooltip,
  Legend,
  Filler,
} from 'chart.js'

console.log('Registering Chart.js components...', Chart)

Chart.register(
  CategoryScale,
  LinearScale,
  BarElement,
  LineElement,
  PointElement,
  BarController,
  LineController,
  Title,
  Tooltip,
  Legend,
  Filler
)

console.log('Chart.js registered successfully', Chart)

// Also make it available globally for debugging
if (typeof window !== 'undefined') {
  (window as any).Chart = Chart
  console.log('Chart.js attached to window')
}

export { Chart }
