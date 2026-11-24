import { BarRow, KpiGrid, KpiTile, Pill, SvCard } from "./kit/system";
import { compactCount, languageRows } from "./repoModel";

/**
 * Analysis data structure returned by the Worker.
 */
interface RepoAnalysis {
  repo: string;
  visibility: string;
  description: string;
  stars: number;
  forks: number;
  totalFiles: number;
  languages: { name: string; bytes: number; percentage: number }[];
  frameworks: { name: string; category: string }[];
  databases: { name: string; evidence: string }[];
  features: string[];
  tests: boolean;
  ci: string;
  docker: boolean;
  architecture: string;
  defaultBranch: string;
}

/**
 * Repository analysis, on the shared sidebar kit (docs/features/197): four number tiles, an About card, the languages as bars,
 * frameworks and databases as lists, and the quality facts as pills. Renders when the user says "analyse owner/repo".
 */
export function AnalysisDashboard({ data }: { data: RepoAnalysis }) {
  const langs = languageRows(data.languages);
  const hasDescription = Boolean(data.description && data.description !== "No description provided.");
  return (
    <>
      <KpiGrid>
        <KpiTile label="Stars" value={compactCount(data.stars)} />
        <KpiTile label="Forks" value={compactCount(data.forks)} />
        <KpiTile label="Files" value={compactCount(data.totalFiles)} />
        <KpiTile label="Branch" value={data.defaultBranch || "–"} />
      </KpiGrid>

      <SvCard title="About" right={<Pill tone="outline" icon={data.visibility === "private" ? "shield" : "globe"}>{data.visibility === "private" ? "Private" : "Public"}</Pill>}>
        {hasDescription ? <p className="sv-lead">{data.description}</p> : <p className="sv-p">No description provided.</p>}
        {data.architecture ? <p className="sv-p" style={{ marginTop: 10 }}>{data.architecture}</p> : null}
      </SvCard>

      {langs.length > 0 && (
        <SvCard title="Languages">
          <div className="sv-bars">
            {langs.map((l) => (
              <BarRow key={l.name} label={l.name} value={`${l.percent} %`} percent={l.percent} tone={l.tone} />
            ))}
          </div>
        </SvCard>
      )}

      {data.frameworks && data.frameworks.length > 0 && (
        <SvCard title="Frameworks & tools">
          <div className="sv-row">
            {data.frameworks.map((f) => (
              <Pill key={f.name} icon="layers">
                {f.name}
              </Pill>
            ))}
          </div>
        </SvCard>
      )}

      {data.databases && data.databases.length > 0 && (
        <SvCard title="Databases">
          <ul className="sv-list">
            {data.databases.map((db) => (
              <li className="sv-li" key={db.name}>
                <div className="sv-grow">
                  <b className="sv-li-t">{db.name}</b>
                  <p className="sv-p">{db.evidence}</p>
                </div>
              </li>
            ))}
          </ul>
        </SvCard>
      )}

      {data.features && data.features.length > 0 && (
        <SvCard title="Features">
          <ul className="sv-list">
            {data.features.map((feature, i) => (
              <li className="sv-li" key={i}>
                <span className="sv-num" aria-hidden="true">{i + 1}</span>
                <div className="sv-grow">
                  <p className="sv-p" style={{ color: "var(--sv-text)" }}>{feature}</p>
                </div>
              </li>
            ))}
          </ul>
        </SvCard>
      )}

      <SvCard title="Quality & CI">
        <div className="sv-row">
          <Pill tone={data.tests ? "solid" : "dash"} icon={data.tests ? "check" : "x"}>
            Tests {data.tests ? "yes" : "no"}
          </Pill>
          <Pill tone={data.ci && data.ci !== "none" ? "solid" : "dash"} icon={data.ci && data.ci !== "none" ? "check" : "x"}>
            CI: {data.ci && data.ci !== "none" ? data.ci : "none"}
          </Pill>
          <Pill tone={data.docker ? "solid" : "dash"} icon={data.docker ? "check" : "x"}>
            Docker {data.docker ? "yes" : "no"}
          </Pill>
        </div>
      </SvCard>
    </>
  );
}
