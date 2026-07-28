inherit("MrxTaskContractOutpost")

function GetOutpostConfig()
  return {
    sOutpostBldg = "AllJob001_01_Outpost",
    tCapturePts = {
      "AllJob001_01_CapturePt1"
    },
    sStagingLayer = "Vz_State_AllJob001_01_Staging",
    sStagingTgLayer = "Vz_State_AllCon050_Tg",
    sPristineLayer = "Vz_State_AllJob001_01_Pristine",
    sDefenseLayer = "Vz_State_AllJob001_01_Defenses",
    sCapturedLayer = "Vz_State_AllJob001_01_Captured",
    sCapturedTgLayer = "Vz_State_AllCon050c_Tg",
    sRivalFaction = "Vza",
    nStartingHealth = 6,
    nRusherQuota = 1
  }
end

function Activated(self)
  MrxTaskContractOutpost.Activated(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-All050-02",
    0.5,
    {
      mattias = "Mattias-In-Mission-Contract-All050-06",
      jennifer = "Jennifer-In-Mission-Contract-All050-07",
      chris = "Chris-In-Mission-Contract-All050-08"
    }
  })
end
