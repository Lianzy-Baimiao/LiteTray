//! Two-phase taskbar clock activation. Prepare captures originals but performs
//! no writes; only an authenticated Start sent to its endpoint can activate it.
use crate::{
    HostLayout, MAX_REQUEST, VERSION,
    clock::Owner,
    routing::{Options, Plan, Route},
};
use serde::{Deserialize, Serialize};
pub const PREPARE_TAG: usize = 0x4c544110;
pub const REPLY_TAG: usize = 0x4c544111;
pub const STATUS_TAG: usize = 0x4c544112;
pub const RECEIVER_CLASS: &str = "LiteTray.TaskbarClockReceiver.v1";
pub const TASKBAR_HASH: &str = "ca1e335ac1fae5ff57519035302a2f365c4620d3a230f31f1231af7331760741";
pub const SHELL_HASH: &str = "8ba440eded430a2417692a98b34695a12ad0ee000333dfed07e0c2723e17111c";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prepare {
    pub version: u32,
    pub owner: Owner,
    pub layout: HostLayout,
    pub routes: Vec<Route>,
    pub custom_calendar: bool,
}
impl Prepare {
    pub fn plan(&self) -> Result<Plan, String> {
        if self.version != VERSION || self.routes.is_empty() {
            return Err("任务栏准备请求无效".into());
        }
        self.owner.validate()?;
        self.layout.validate()?;
        Plan::build(
            &self.routes,
            &self
                .routes
                .iter()
                .map(|r| r.device.clone())
                .collect::<Vec<_>>(),
            Options {
                clocks: true,
                tray: false,
                icons: false,
                control_center: false,
                copied_taskbars_only: false,
            },
        )
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > MAX_REQUEST {
            return Err("任务栏准备请求尺寸无效".into());
        }
        let request: Self = serde_json::from_slice(bytes).map_err(|_| "任务栏准备请求格式无效")?;
        request.plan()?;
        Ok(request)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub version: u32,
    pub owner: Owner,
    pub endpoint: u64,
    pub targets: Vec<(u64, u64)>,
    pub error: Option<String>,
}
impl Reply {
    pub fn validate(&self, request: &Prepare) -> Result<(), String> {
        if self.version != VERSION || self.owner != request.owner {
            return Err("任务栏准备响应身份不匹配".into());
        }
        if let Some(error) = &self.error {
            if self.endpoint != 0 || !self.targets.is_empty() || error.len() > 4096 {
                return Err("任务栏错误响应无效".into());
            }
            return Err(error.clone());
        }
        if self.endpoint == 0 || self.targets.len() != request.routes.len() {
            return Err("任务栏响应缺少目标或端点".into());
        }
        crate::activation::Gate::new(self.owner.clone(), self.targets.clone())?;
        if self
            .targets
            .iter()
            .any(|(clock, _)| !request.routes.iter().any(|r| r.window == *clock))
        {
            return Err("任务栏响应含未请求的时钟".into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Prepare {
        Prepare {
            version: VERSION,
            owner: Owner {
                nonce: 7,
                process_id: 2,
                receiver: 3,
            },
            layout: crate::tests::layout(),
            routes: vec![Route {
                window: 11,
                process_id: 12,
                thread_id: 13,
                secondary: false,
                device: "DISPLAY1".into(),
            }],
            custom_calendar: true,
        }
    }
    #[test]
    fn preparation_is_bounded_and_rejects_ambiguous_or_cross_thread_targets() {
        let mut request = request();
        let bytes = serde_json::to_vec(&request).unwrap();
        let decoded = Prepare::decode(&bytes).unwrap();
        let plan = decoded.plan().unwrap();
        assert!(
            plan.source.is_none() && plan.targets[0].clock && plan.targets[0].models.is_empty()
        );
        request.routes.push(request.routes[0].clone());
        assert!(request.plan().is_err());
        request.routes[1].window = 14;
        request.routes[1].device = "DISPLAY2".into();
        request.routes[1].thread_id = 15;
        assert!(request.plan().is_err());
        request.routes.clear();
        assert!(request.plan().is_err());
        assert!(Prepare::decode(&vec![0; MAX_REQUEST + 1]).is_err());
        let mut value = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
        value["frame"] = serde_json::json!({"top":"unexpected activation"});
        assert!(Prepare::decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    #[test]
    fn reply_must_identify_exact_owner_endpoint_and_target_set() {
        let request = request();
        let reply = Reply {
            version: VERSION,
            owner: request.owner.clone(),
            endpoint: 21,
            targets: vec![(11, 22)],
            error: None,
        };
        reply.validate(&request).unwrap();
        let mut wrong = reply.clone();
        wrong.owner.nonce += 1;
        assert!(wrong.validate(&request).is_err());
        let mut wrong = reply.clone();
        wrong.targets = vec![(99, 22)];
        assert!(wrong.validate(&request).is_err());
        let mut wrong = reply.clone();
        wrong.endpoint = 0;
        assert!(wrong.validate(&request).is_err());
        let mut wrong = reply.clone();
        wrong.targets.push((11, 22));
        assert!(wrong.validate(&request).is_err());
        let wrong = Reply {
            error: Some("capture failed".into()),
            endpoint: 0,
            targets: vec![],
            ..reply
        };
        assert_eq!(wrong.validate(&request).unwrap_err(), "capture failed");
    }
}
