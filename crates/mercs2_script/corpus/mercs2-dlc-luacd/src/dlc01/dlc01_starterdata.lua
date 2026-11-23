local L0_1, L1_1, L2_1, L3_1, L4_1
L0_1 = {}
L0_1.bFemale = true
L0_1.sFaceFxSet = "Global_Job_Briefing_Fiona"
HubFiona = L0_1
L0_1 = {}
L0_1.sFaceFxSet = "Global_Job_Briefing_Ewan"
HubEwan = L0_1
L0_1 = {}
L0_1.bFemale = true
L0_1.sFaceFxSet = "Global_Job_Briefing_Eva"
HubEva = L0_1
L0_1 = {}
L0_1.sFaceFxSet = "Global_Job_Briefing_Misha"
HubMisha = L0_1
L0_1 = {}
L0_1[1] = "HubFiona"
L0_1[2] = "HubEwan"
L0_1[3] = "HubEva"
L0_1[4] = "HubMisha"
_sStarters = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2
  L0_2 = {}
  L1_2 = {}
  L3_2 = HubMisha
  L1_2[1] = HubEwan
  L1_2[2] = L3_2
  L1_2[3] = HubEva
  L1_2[4] = HubFiona
  L0_2.Pmc = L1_2
  L1_2 = pairs
  L2_2 = L0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = ipairs
    L7_2 = L5_2
    L6_2, L7_2, L8_2 = L6_2(L7_2)
    for L9_2, L10_2 in L6_2, L7_2, L8_2 do
      L10_2.sFaction = L4_2
      L11_2 = L10_2.sVoBankName
      if L11_2 then
        L12_2 = L10_2.bBoss
        if not L12_2 then
          L12_2 = {}
          L13_2 = {}
          L14_2 = {}
          L15_2 = {}
          L15_2.Chris = (L11_2 .. ".ChrisHappyOnce01")
          L15_2.Jennifer = (L11_2 .. ".JenHappyOnce01")
          L15_2.Mattias = (L11_2 .. ".MattiasHappyOnce01")
          L14_2.Positive = L15_2
          L15_2 = {}
          L15_2.Chris = (L11_2 .. ".ChrisNeutralOnce01")
          L15_2.Jennifer = (L11_2 .. ".JenNeutralOnce01")
          L15_2.Mattias = (L11_2 .. ".MattiasNeutralOnce01")
          L14_2.Neutral = L15_2
          L15_2 = {}
          L15_2.Chris = (L11_2 .. ".ChrisAngryOnce01")
          L15_2.Jennifer = (L11_2 .. ".JenAngryOnce01")
          L15_2.Mattias = (L11_2 .. ".MattiasAngryOnce01")
          L14_2.Negative = L15_2
          L13_2.Initial = L14_2
          L14_2 = {}
          L15_2 = {}
          L15_2[1] = (L11_2 .. ".Happy01")
          L15_2[2] = (L11_2 .. ".Happy02")
          L15_2[3] = (L11_2 .. ".Happy03")
          L14_2.Positive = L15_2
          L15_2 = {}
          L15_2[1] = (L11_2 .. ".Neutral01")
          L15_2[2] = (L11_2 .. ".Neutral02")
          L15_2[3] = (L11_2 .. ".Neutral03")
          L14_2.Neutral = L15_2
          L15_2 = {}
          L15_2[1] = (L11_2 .. ".Angry01")
          L15_2[2] = (L11_2 .. ".Angry02")
          L15_2[3] = (L11_2 .. ".Angry03")
          L14_2.Negative = L15_2
          L13_2.Subsequent = L14_2
          L12_2.Greetings = L13_2
          L12_2.NoJobs = (L11_2 .. ".NoJob01")
          L12_2.JobSummary = (L11_2 .. ".ActiveJob01")
          L13_2 = {}
          L13_2[1] = (L11_2 .. ".Goodbye01")
          L13_2[2] = (L11_2 .. ".Goodbye02")
          L13_2[3] = (L11_2 .. ".Goodbye03")
          L12_2.Goodbyes = L13_2
          L10_2.tBriefingWrapper = L12_2
        end
      end
    end
  end
end

Init = L0_1

function L0_1(A0_2)
  local L1_2
  L1_2 = _THIS[A0_2].sPlayerVisibleName
  return L1_2
end

GetPlayerVisibleName = L0_1
