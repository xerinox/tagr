use crate::vtags::cache::MetadataCache;
use crate::vtags::config::VirtualTagConfig;
use crate::vtags::types::{
    ExtTypeCategory, PermissionCondition, RangeCondition, SizeCategory,
    SizeCondition, TimeCondition, VirtualTag,
};
use chrono::{DateTime, Datelike, Local, Utc};
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, SystemTime};

pub struct VirtualTagEvaluator {
    cache: MetadataCache,
    config: VirtualTagConfig,
}

impl VirtualTagEvaluator {
    #[must_use] 
    pub fn new(cache_ttl: Duration, config: VirtualTagConfig) -> Self {
        Self {
            cache: MetadataCache::new(cache_ttl),
            config,
        }
    }

    pub fn matches(&mut self, path: &Path, vtag: &VirtualTag) -> io::Result<bool> {
        match vtag {
            VirtualTag::Modified(cond) => self.check_time(path, cond, TimeField::Modified),
            VirtualTag::Created(cond) => self.check_time(path, cond, TimeField::Created),
            VirtualTag::Accessed(cond) => self.check_time(path, cond, TimeField::Accessed),
            VirtualTag::Size(cond) => self.check_size(path, cond),
            VirtualTag::Extension(ext) => Ok(self.check_extension(path, ext)),
            VirtualTag::ExtensionType(category) => Ok(self.check_ext_type(path, *category)),
            VirtualTag::Directory(dir) => Ok(self.check_directory(path, dir)),
            VirtualTag::Path(pattern) => Ok(self.check_path_pattern(path, pattern)),
            VirtualTag::Depth(range) => Ok(self.check_depth(path, range)),
            VirtualTag::Permission(perm) => self.check_permission(path, perm),
            VirtualTag::Lines(range) => self.check_lines(path, range),
            VirtualTag::Git(_cond) => Ok(false),
        }
    }

    fn check_time(
        &mut self,
        path: &Path,
        cond: &TimeCondition,
        field: TimeField,
    ) -> io::Result<bool> {
        let metadata = self.cache.get(path)?;
        let file_time = match field {
            TimeField::Modified => metadata.modified,
            TimeField::Created => metadata.created.ok_or_else(|| {
                io::Error::new(io::ErrorKind::Unsupported, "Created time not available")
            })?,
            TimeField::Accessed => metadata.accessed.ok_or_else(|| {
                io::Error::new(io::ErrorKind::Unsupported, "Accessed time not available")
            })?,
        };

        Ok(evaluate_time_condition(file_time, cond))
    }

    fn check_size(&mut self, path: &Path, cond: &SizeCondition) -> io::Result<bool> {
        let metadata = self.cache.get(path)?;
        Ok(self.evaluate_size_condition(metadata.size, cond))
    }

    fn check_extension(&self, path: &Path, ext: &str) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| {
                let ext_with_dot = if ext.starts_with('.') { ext } else { &format!(".{ext}") };
                format!(".{e}") == ext_with_dot
            })
    }

    fn check_ext_type(&self, path: &Path, category: ExtTypeCategory) -> bool {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{e}"));

        if let Some(ext) = ext {
            let category_name = match category {
                ExtTypeCategory::Source => "source",
                ExtTypeCategory::Document => "document",
                ExtTypeCategory::Image => "image",
                ExtTypeCategory::Archive => "archive",
                ExtTypeCategory::Config => "config",
            };

            if let Some(extensions) = self.config.extension_types.get(category_name) {
                return extensions.contains(&ext);
            }
        }

        false
    }

    fn check_directory(&self, path: &Path, dir: &Path) -> bool {
        path.parent()
            .is_some_and(|parent| parent == dir || parent.ends_with(dir))
    }

    fn check_path_pattern(&self, path: &Path, pattern: &glob::Pattern) -> bool {
        pattern.matches_path(path)
    }

    fn check_depth(&self, path: &Path, range: &RangeCondition) -> bool {
        let depth = path.components().count() as u64;
        evaluate_range_condition(depth, range)
    }

    fn check_permission(&mut self, path: &Path, perm: &PermissionCondition) -> io::Result<bool> {
        let metadata = self.cache.get(path)?;
        let mode = metadata.permissions.mode();

        Ok(match perm {
            PermissionCondition::Executable => mode & 0o111 != 0,
            PermissionCondition::Readable => mode & 0o444 != 0,
            PermissionCondition::Writable => mode & 0o222 != 0,
            PermissionCondition::ReadOnly => mode & 0o222 == 0,
        })
    }

    fn check_lines(&mut self, path: &Path, range: &RangeCondition) -> io::Result<bool> {
        let metadata = self.cache.get(path)?;
        if !metadata.is_file {
            return Ok(false);
        }

        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let line_count = reader.lines().count() as u64;

        Ok(evaluate_range_condition(line_count, range))
    }

    fn evaluate_size_condition(&self, size: u64, cond: &SizeCondition) -> bool {
        match cond {
            SizeCondition::Empty => size == 0,
            SizeCondition::Category(cat) => {
                let (min, max) = match cat {
                    SizeCategory::Tiny => (0, self.config.get_size_threshold("tiny").unwrap_or(1024)),
                    SizeCategory::Small => (
                        self.config.get_size_threshold("tiny").unwrap_or(1024),
                        self.config.get_size_threshold("small").unwrap_or(102_400),
                    ),
                    SizeCategory::Medium => (
                        self.config.get_size_threshold("small").unwrap_or(102_400),
                        self.config.get_size_threshold("medium").unwrap_or(1_048_576),
                    ),
                    SizeCategory::Large => (
                        self.config.get_size_threshold("medium").unwrap_or(1_048_576),
                        self.config.get_size_threshold("large").unwrap_or(10_485_760),
                    ),
                    SizeCategory::Huge => (self.config.get_size_threshold("large").unwrap_or(10_485_760), u64::MAX),
                };
                size >= min && size < max
            }
            SizeCondition::GreaterThan(threshold) => size > *threshold,
            SizeCondition::LessThan(threshold) => size < *threshold,
            SizeCondition::Equals(target) => size == *target,
            SizeCondition::Range(min, max) => size >= *min && size <= *max,
        }
    }
}

enum TimeField {
    Modified,
    Created,
    Accessed,
}

fn evaluate_time_condition(file_time: SystemTime, cond: &TimeCondition) -> bool {
    let now = SystemTime::now();
    let local_now = Local::now();

    match cond {
        TimeCondition::Today => {
            let today_start = local_now.date_naive().and_hms_opt(0, 0, 0).unwrap();
            let today_start = today_start
                .and_local_timezone(Local)
                .single()
                .unwrap()
                .with_timezone(&Utc);
            let file_datetime: DateTime<Utc> = file_time.into();
            file_datetime >= today_start
        }
        TimeCondition::Yesterday => {
            let yesterday = local_now.date_naive().pred_opt().unwrap();
            let yesterday_start = yesterday.and_hms_opt(0, 0, 0).unwrap();
            let yesterday_start = yesterday_start
                .and_local_timezone(Local)
                .single()
                .unwrap()
                .with_timezone(&Utc);
            let today_start = local_now.date_naive().and_hms_opt(0, 0, 0).unwrap();
            let today_start = today_start
                .and_local_timezone(Local)
                .single()
                .unwrap()
                .with_timezone(&Utc);
            let file_datetime: DateTime<Utc> = file_time.into();
            file_datetime >= yesterday_start && file_datetime < today_start
        }
        TimeCondition::ThisWeek => {
            let week_start = local_now.date_naive().week(chrono::Weekday::Mon).first_day();
            let week_start = week_start
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_local_timezone(Local)
                .single()
                .unwrap()
                .with_timezone(&Utc);
            let file_datetime: DateTime<Utc> = file_time.into();
            file_datetime >= week_start
        }
        TimeCondition::ThisMonth => {
            let month_start = local_now
                .date_naive()
                .with_day(1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let month_start = month_start
                .and_local_timezone(Local)
                .single()
                .unwrap()
                .with_timezone(&Utc);
            let file_datetime: DateTime<Utc> = file_time.into();
            file_datetime >= month_start
        }
        TimeCondition::ThisYear => {
            let year_start = local_now
                .date_naive()
                .with_month(1)
                .unwrap()
                .with_day(1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let year_start = year_start
                .and_local_timezone(Local)
                .single()
                .unwrap()
                .with_timezone(&Utc);
            let file_datetime: DateTime<Utc> = file_time.into();
            file_datetime >= year_start
        }
        TimeCondition::LastNDays(n) => {
            let threshold = now
                .checked_sub(std::time::Duration::from_secs(u64::from(*n) * 86400))
                .unwrap();
            file_time >= threshold
        }
        TimeCondition::LastNHours(n) => {
            let threshold = now
                .checked_sub(std::time::Duration::from_secs(u64::from(*n) * 3600))
                .unwrap();
            file_time >= threshold
        }
        TimeCondition::After(date) => {
            let file_datetime: DateTime<Utc> = file_time.into();
            file_datetime >= *date
        }
        TimeCondition::Before(date) => {
            let file_datetime: DateTime<Utc> = file_time.into();
            file_datetime < *date
        }
        TimeCondition::Between(start, end) => {
            let file_datetime: DateTime<Utc> = file_time.into();
            file_datetime >= *start && file_datetime < *end
        }
    }
}

const fn evaluate_range_condition(value: u64, cond: &RangeCondition) -> bool {
    match cond {
        RangeCondition::Equals(target) => value == *target,
        RangeCondition::GreaterThan(threshold) => value > *threshold,
        RangeCondition::LessThan(threshold) => value < *threshold,
        RangeCondition::Range(min, max) => value >= *min && value <= *max,
    }
}
