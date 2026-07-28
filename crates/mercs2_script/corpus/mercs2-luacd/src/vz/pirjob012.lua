inherit("MrxTaskJobVerifySet")
import("MrxVoSequence")

function LoadAssets(self, tSaveData)
  self:_AddTarget({
    sTarget = "PirJob012_Target_01",
    sDefenseLayer = "Vz_State_PirJob012_01",
    sPristineLayer = "Vz_State_PirJob012_01_Pristine",
    sStagingLayer = "Vz_State_PirJob012_01_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Pir12-01"
  })
  self:_AddTarget({
    sTarget = "PirJob012_Target_02",
    sDefenseLayer = "Vz_State_PirJob012_02",
    sPristineLayer = "Vz_State_PirJob012_02_Pristine",
    sStagingLayer = "Vz_State_PirJob012_02_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Pir12-16"
  })
  self:_AddTarget({
    sTarget = "PirJob012_Target_03",
    sDefenseLayer = "Vz_State_PirJob012_03",
    sPristineLayer = "Vz_State_PirJob012_03_Pristine",
    sStagingLayer = "Vz_State_PirJob012_03_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Pir12-02"
  })
  self:_AddTarget({
    sTarget = "PirJob012_Target_04",
    sDefenseLayer = "Vz_State_PirJob012_04",
    sStagingLayer = "Vz_State_PirJob012_04_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Pir12-17"
  })
  self:_AddTarget({
    sTarget = "PirJob012_Target_05",
    sDefenseLayer = "Vz_State_PirJob012_05",
    sPristineLayer = "Vz_State_PirJob012_05_Pristine",
    sStagingLayer = "Vz_State_PirJob012_05_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Pir12-19"
  })
  self:_AddTarget({
    sTarget = "PirJob012_Target_06",
    sDefenseLayer = "Vz_State_PirJob012_06",
    sPristineLayer = "Vz_State_PirJob012_06_Pristine",
    sStagingLayer = "Vz_State_PirJob012_06_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Pir12-05"
  })
  self:_AddTarget({
    sTarget = "PirJob012_Target_07",
    sDefenseLayer = "Vz_State_PirJob012_07",
    sPristineLayer = "Vz_State_PirJob012_07_Pristine",
    sStagingLayer = "Vz_State_PirJob012_07_Staging",
    vNearVoSequence = "Fiona.Brian.05"
  })
  self:_AddTarget({
    sTarget = "PirJob012_Target_08",
    sDefenseLayer = "Vz_State_PirJob012_08",
    sPristineLayer = "Vz_State_PirJob012_08_Pristine",
    sStagingLayer = "Vz_State_PirJob012_08_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Pir12-21"
  })
  self:_AddTarget({
    sTarget = "PirJob012_Target_09",
    sDefenseLayer = "Vz_State_PirJob012_09",
    sPristineLayer = "Vz_State_PirJob012_09_Pristine",
    sStagingLayer = "Vz_State_PirJob012_09_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Pir12-10"
  })
  self:_AddTarget({
    sTarget = "PirJob012_Target_10",
    sDefenseLayer = "Vz_State_PirJob012_10",
    sPristineLayer = "Vz_State_PirJob012_10_Pristine",
    sStagingLayer = "Vz_State_PirJob012_10_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Pir12-23"
  })
  MrxTaskJobVerifySet.LoadAssets(self, tSaveData)
end

function Activated(self)
  MrxTaskJobVerifySet.Activated(self)
  self:_SetFactionId("Pir")
  self:_SetTargetNearbyVo(_tTargetNearbyVo)
  self:_SetTargetCompleteVo(_tTargetCompleteVo)
  self:_Go()
end

function Cleanup(self)
  MrxTaskJobVerifySet.Cleanup(self)
end
