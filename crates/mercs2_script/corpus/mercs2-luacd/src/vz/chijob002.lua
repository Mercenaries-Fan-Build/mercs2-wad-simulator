inherit("MrxTaskJobVerifySet")
import("MrxVoSequence")
_tTargetCompleteVo = {
  {
    vSequence = "Fiona-In-Mission-Job-Chi10-06"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Chi10-07"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Chi10-08"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Chi10-09",
    tRange = {9}
  },
  {
    vSequence = "Fiona-In-Mission-Job-Chi10-10"
  }
}

function LoadAssets(self, tSaveData)
  self:_AddTarget({
    sTarget = "ChiJob002_Target_01",
    sDefenseLayer = "Vz_State_ChiJob002_01",
    sPristineLayer = "Vz_State_ChiJob002_01_Pristine",
    sStagingLayer = "Vz_State_ChiJob002_01_Staging",
    sVerifiedLayer = "Vz_State_ChiJob002_01_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Chi02-06"
  })
  self:_AddTarget({
    sTarget = "ChiJob002_Target_02",
    sDefenseLayer = "Vz_State_ChiJob002_02",
    sPristineLayer = "Vz_State_ChiJob002_02_Pristine",
    sStagingLayer = "Vz_State_ChiJob002_02_Staging",
    sVerifiedLayer = "Vz_State_ChiJob002_02_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Chi02-07"
  })
  self:_AddTarget({
    sTarget = "ChiJob002_Target_03",
    sDefenseLayer = "Vz_State_ChiJob002_03",
    sPristineLayer = "Vz_State_ChiJob002_03_Pristine",
    sStagingLayer = "Vz_State_ChiJob002_02_Staging",
    sVerifiedLayer = "Vz_State_ChiJob002_03_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Chi02-08"
  })
  self:_AddTarget({
    sTarget = "ChiJob002_Target_04",
    sDefenseLayer = "Vz_State_ChiJob002_04",
    sPristineLayer = "Vz_State_ChiJob002_04_Pristine",
    sStagingLayer = "Vz_State_ChiJob002_04_Staging",
    sVerifiedLayer = "Vz_State_ChiJob002_02_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Chi02-09"
  })
  self:_AddTarget({
    sTarget = "ChiJob002_Target_05",
    sDefenseLayer = "Vz_State_ChiJob002_05",
    sPristineLayer = "Vz_State_ChiJob002_05_Pristine",
    sStagingLayer = "Vz_State_ChiJob002_05_Staging",
    sVerifiedLayer = "Vz_State_ChiJob002_02_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Chi02-10"
  })
  self:_AddTarget({
    sTarget = "ChiJob010_Target_01",
    sDefenseLayer = "Vz_State_ChiJob010_01",
    sPristineLayer = "Vz_State_ChiJob010_01_Pristine",
    sStagingLayer = "Vz_State_ChiJob010_01_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Chi10-02"
  })
  self:_AddTarget({
    sTarget = "ChiJob010_Target_02",
    sDefenseLayer = "Vz_State_ChiJob010_02",
    sPristineLayer = "Vz_State_ChiJob010_02_Pristine",
    sStagingLayer = "Vz_State_ChiJob010_02_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Chi10-01"
  })
  self:_AddTarget({
    sTarget = "ChiJob010_Target_03",
    sDefenseLayer = "Vz_State_ChiJob010_03",
    sStagingLayer = "Vz_State_ChiJob010_03_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Chi10-13"
  })
  self:_AddTarget({
    sTarget = "ChiJob010_Target_04",
    sDefenseLayer = "Vz_State_ChiJob010_04",
    sPristineLayer = "Vz_State_ChiJob010_04_Pristine",
    sStagingLayer = "Vz_State_ChiJob010_04_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Chi10-14"
  })
  self:_AddTarget({
    sTarget = "ChiJob010_Target_05",
    sDefenseLayer = "Vz_State_ChiJob010_05",
    sPristineLayer = "Vz_State_ChiJob009_B_Pristine",
    sStagingLayer = "Vz_State_ChiJob010_05_Staging",
    vNearVoSequence = "Fiona-In-Mission-Job-Chi10-15"
  })
  MrxLayerManager.MarkForAddition("Vz_State_ChiJob009_B_Pristine_tg")
  MrxTaskJobVerifySet.LoadAssets(self, tSaveData)
end

function Activated(self)
  MrxTaskJobVerifySet.Activated(self)
  self:_SetFactionId("Chi")
  self:_SetTargetCompleteVo(_tTargetCompleteVo)
  self:_Go()
end

function Cleanup(self)
  MrxTaskJobVerifySet.Cleanup(self)
end
