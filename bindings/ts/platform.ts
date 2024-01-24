/*
 Generated by typeshare 1.7.0
*/

export enum CPUGovernor {
	Performance = "Performance",
	Powersave = "Powersave",
	BadValue = "BadValue",
}

export enum CPUEPP {
	Default = "Default",
	Performance = "Performance",
	BalancePerformance = "BalancePerformance",
	BalancePower = "BalancePower",
	Power = "Power",
}

export enum GpuMode {
	Discrete = "Discrete",
	Optimus = "Optimus",
	Integrated = "Integrated",
	Egpu = "Egpu",
	Vfio = "Vfio",
	Ultimate = "Ultimate",
	Error = "Error",
	NotSupported = "NotSupported",
}

/** `throttle_thermal_policy` in asus_wmi */
export enum ThrottlePolicy {
	Balanced = "Balanced",
	Performance = "Performance",
	Quiet = "Quiet",
}

/** CamelCase names of the properties. Intended for use with DBUS */
export enum Properties {
	ChargeControlEndThreshold = "ChargeControlEndThreshold",
	DgpuDisable = "DgpuDisable",
	GpuMuxMode = "GpuMuxMode",
	PostAnimationSound = "PostAnimationSound",
	PanelOd = "PanelOd",
	MiniLedMode = "MiniLedMode",
	EgpuEnable = "EgpuEnable",
	ThrottlePolicy = "ThrottlePolicy",
	PptPl1Spl = "PptPl1Spl",
	PptPl2Sppt = "PptPl2Sppt",
	PptFppt = "PptFppt",
	PptApuSppt = "PptApuSppt",
	PptPlatformSppt = "PptPlatformSppt",
	NvDynamicBoost = "NvDynamicBoost",
	NvTempTarget = "NvTempTarget",
}

