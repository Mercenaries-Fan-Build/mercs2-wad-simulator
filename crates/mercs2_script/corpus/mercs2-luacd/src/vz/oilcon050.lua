inherit("MrxTaskContractOutpost")
import("MrxVoSequence")
import("WifMissionFlow")

function GetOutpostConfig()
  return {
    sOutpostBldg = "OilJob001_Outpost",
    tCapturePts = {
      "OilJob001_CapturePt1"
    },
    sStagingLayer = "Vz_State_OilJob001_Staging",
    sStagingTgLayer = "Vz_State_OilCon050_Tg",
    sPristineLayer = "Vz_State_OilJob001_Pristine",
    sDefenseLayer = "Vz_State_OilJob001_Defenses",
    sCapturedLayer = "Vz_State_OilJob001_Captured",
    sCapturedTgLayer = "Vz_State_OilCon050c_Tg",
    sRivalFaction = "Vza",
    nStartingHealth = 3,
    nRusherQuota = 1
  }
end

function Activated(self)
  MrxTaskContractOutpost.Activated(self)
  if not WifMissionFlow.HasKey("GurCon053") then
    self:_CreateEvent(Event.ObjectProximity, {
      Player.GetAnyCharacter(),
      Pg.GetGuidByName("OilJob001_Outpost"),
      "<",
      100,
      false,
      false
    }, NearOutpost, {self})
    OilCon050_FionaVO_Activate(self)
  end
end

function OilCon050_FionaVO_Activate(self)
  MrxVoSequence.Start({
    "Fiona.Misc.Outposts03",
    0.5,
    "Fiona.Misc.Outposts04",
    0.5,
    "Fiona.Misc.Outposts06"
  })
end

function NearOutpost(self)
  MrxVoSequence.Start({
    "Fiona.Misc.Outposts07"
  })
end
