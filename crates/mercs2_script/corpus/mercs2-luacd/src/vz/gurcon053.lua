inherit("MrxTaskContractOutpost")

function GetOutpostConfig()
  return {
    sOutpostBldg = "GurJob008_02_Outpost",
    tCapturePts = {
      "GurJob008_02_CapturePt3"
    },
    sStagingLayer = "Vz_State_GurJob008_02_Staging",
    sStagingTgLayer = "Vz_State_GurCon053_Tg",
    sPristineLayer = "Vz_State_GurJob008_02_Pristine",
    sDefenseLayer = "Vz_State_GurJob008_02_Defenses",
    sCapturedLayer = "Vz_State_GurJob008_02_Captured",
    sCapturedTgLayer = "Vz_State_GurCon053c_Tg",
    sRivalFaction = "Vza",
    nStartingHealth = 3,
    nRusherQuota = 1
  }
end

function Activated(self)
  MrxTaskContractOutpost.Activated(self)
  if not WifMissionFlow.HasKey("OilCon050") then
    self:_CreateEvent(Event.ObjectProximity, {
      Player.GetAnyCharacter(),
      Pg.GetGuidByName("GurJob008_02_Outpost"),
      "<",
      100,
      false,
      false
    }, NearOutpost, {self})
    GurCon053_FionaVO_Activate_PreOil(self)
  else
    GurCon053_FionaVO_Activate_PostOil(self)
  end
end

function GurCon053_FionaVO_Activate_PreOil(self)
  MrxVoSequence.Start({
    "Fiona.Misc.Outposts02",
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

function GurCon053_FionaVO_Activate_PostOil(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Gur053-01"
  })
end
