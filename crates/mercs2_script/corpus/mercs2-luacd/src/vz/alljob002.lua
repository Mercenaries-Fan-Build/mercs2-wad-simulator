inherit("MrxTaskJobVerifySet")
import("MrxVoSequence")
_tTargetNearbyVo = {
  {
    vSequence = "Fiona-In-Mission-Job-All10-03"
  },
  {
    vSequence = "Fiona-In-Mission-Job-All10-12"
  },
  {
    vSequence = "Fiona-In-Mission-Job-All10-13"
  }
}
_tTargetCompleteVo = {
  {
    vSequence = "Fiona-In-Mission-Job-All10-06"
  },
  {
    vSequence = "Fiona-In-Mission-Job-All10-08"
  },
  {
    vSequence = "Fiona-In-Mission-Job-All10-09"
  },
  {
    vSequence = "Fiona-In-Mission-Job-All10-10"
  },
  {},
  {},
  {}
}

function LoadAssets(self, tSaveData)
  self:_AddTarget({
    sTarget = "AllJob002_01_Target",
    sDefenseLayer = "vz_State_AllJob002_01_Defenses",
    sStagingLayer = "vz_State_AllJob002_01_staging",
    sPristineLayer = "vz_State_AllJob002_01_pristine",
    sVerifiedLayer = "vz_State_AllJob002_01_staging",
    vNearVoSequence = "Fiona-In-Mission-Job-All02-01"
  })
  self:_AddTarget({
    sTarget = "AllJob002_02_Target",
    sDefenseLayer = "vz_State_AllJob002_02_Defenses",
    sStagingLayer = "vz_State_AllJob002_02_staging",
    sPristineLayer = "vz_State_AllJob002_02_pristine",
    sVerifiedLayer = "vz_State_AllJob002_02_staging",
    vNearVoSequence = "Fiona-In-Mission-Job-All02-02"
  })
  self:_AddTarget({
    sTarget = "AllJob002_03_Target",
    sDefenseLayer = "vz_State_AllJob002_03_Defenses",
    sStagingLayer = "vz_State_AllJob002_03_staging",
    sPristineLayer = "vz_State_AllJob002_03_pristine",
    sVerifiedLayer = "vz_State_AllJob002_03_staging",
    vNearVoSequence = "Fiona-In-Mission-Job-All02-03"
  })
  self:_AddTarget({
    sTarget = "AllJob002_04_Target",
    sDefenseLayer = "vz_State_AllJob002_04_Defenses",
    sStagingLayer = "vz_State_AllJob002_04_staging",
    sPristineLayer = "vz_State_AllJob002_04_pristine",
    sVerifiedLayer = "vz_State_AllJob002_04_staging"
  })
  self:_AddTarget({
    sTarget = "AllJob002_05_Target",
    sDefenseLayer = "vz_State_AllJob002_05_Defenses",
    sStagingLayer = "vz_State_AllJob002_05_staging",
    sPristineLayer = "vz_State_AllJob002_05_pristine",
    sVerifiedLayer = "vz_State_AllJob002_05_staging"
  })
  self:_AddTarget({
    sTarget = "AllJob010_01_Target",
    sDefenseLayer = "vz_State_AllJob010_01",
    sStagingLayer = "vz_State_AllJob010_01_staging"
  })
  self:_AddTarget({
    sTarget = "AllJob010_02_Target",
    sDefenseLayer = "vz_State_AllJob010_02",
    sStagingLayer = "vz_State_AllJob010_02_staging",
    sPristineLayer = "vz_State_AllJob010_02_pristine",
    vNearVoSequence = "Fiona-In-Mission-Job-All10-02"
  })
  self:_AddTarget({
    sTarget = "AllJob010_03_Target",
    sDefenseLayer = "vz_State_AllJob010_03",
    sStagingLayer = "vz_State_AllJob010_03_staging",
    sPristineLayer = "vz_State_AllJob010_03_pristine"
  })
  self:_AddTarget({
    sTarget = "AllJob010_04_Target",
    sDefenseLayer = "vz_State_AllJob010_04",
    sStagingLayer = "vz_State_AllJob010_04_staging",
    sPristineLayer = "vz_State_AllJob010_04_pristine",
    vNearVoSequence = "Fiona-In-Mission-Job-All10-04"
  })
  self:_AddTarget({
    sTarget = "AllJob010_05_Target",
    sDefenseLayer = "vz_State_AllJob010_05",
    sStagingLayer = "vz_State_AllJob010_05_staging",
    sPristineLayer = "vz_State_AllJob010_05_pristine",
    vNearVoSequence = "Fiona-In-Mission-Job-All10-05"
  })
  MrxTaskJobVerifySet.LoadAssets(self, tSaveData)
end

function Activated(self)
  MrxTaskJobVerifySet.Activated(self)
  self:JeepRegionActivate()
  self:_SetFactionId("All")
  self:_SetTargetNearbyVo(_tTargetNearbyVo)
  self:_SetTargetCompleteVo(_tTargetCompleteVo)
  self:_Go()
end

function JeepRegionActivate(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_AllJob002_04_TriggerJeep"),
    "enter",
    false
  }, JeepAssault, {self})
end

function JeepAssault(self)
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName("AllJob002_04_Jeep01")),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("Path_AllJob002_Jeep01"),
    Mode = "Oneway",
    Priority = "hiPri",
    Haste = 0.7
  })
end

function Cleanup(self)
  MrxTaskJobVerifySet.Cleanup(self)
end
