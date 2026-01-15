export type BarChartProps = {
  values: number[];
  height?: number;
  maxBars?: number;
  color?: string;
  background?: string;
};

function downsample(values: number[], maxBars: number): number[] {
  if (values.length <= maxBars) return values;
  const stride = Math.ceil(values.length / maxBars);
  const out: number[] = [];
  for (let i = 0; i < values.length; i += stride) out.push(values[i]!);
  return out;
}

export function BarChart({
  values,
  height = 120,
  maxBars = 200,
  color = "#2563eb",
  background = "rgba(2, 6, 23, 0.04)",
}: BarChartProps) {
  const data = downsample(values, maxBars);
  const vbWidth = 1000;
  const vbHeight = 100;

  const max = Math.max(1, ...data.filter((v) => Number.isFinite(v) && v >= 0));
  const barW = data.length > 0 ? vbWidth / data.length : vbWidth;

  return (
    <div style={{ display: "grid", gap: 8 }}>
      <svg
        viewBox={`0 0 ${vbWidth} ${vbHeight}`}
        preserveAspectRatio="none"
        style={{ width: "100%", height, background, borderRadius: 10 }}
      >
        {data.length === 0 ? null : (
          <>
            {data.map((v, i) => {
              const clamped = Number.isFinite(v) && v > 0 ? v : 0;
              const h = (clamped / max) * vbHeight;
              const x = i * barW;
              const y = vbHeight - h;
              return (
                <rect
                  key={i}
                  x={x}
                  y={y}
                  width={barW * 0.9}
                  height={h}
                  fill={color}
                  opacity={0.9}
                  rx={barW > 8 ? 2 : 0}
                />
              );
            })}
          </>
        )}
      </svg>
      <div
        className="muted"
        style={{ display: "flex", justifyContent: "space-between", gap: 12 }}
      >
        <div>
          Bars: <span className="mono">{data.length}</span>
          {data.length !== values.length ? (
            <span>
              {" "}
              (downsampled from <span className="mono">{values.length}</span>)
            </span>
          ) : null}
        </div>
        <div>
          Max: <span className="mono">{max}</span> ms
        </div>
      </div>
    </div>
  );
}
