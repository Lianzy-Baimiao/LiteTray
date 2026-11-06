// Leap month numbers from .NET runtime ChineseLunisolarCalendar (MIT).
// https://github.com/dotnet/runtime/blob/main/src/libraries/System.Private.CoreLib/src/System/Globalization/ChineseLunisolarCalendar.cs
// Source SHA-256: f6a6a3e586571e6afeaa347ad26d7b9ade0a2a160667b62539feac4c6cf7ada6
// Copyright (c) .NET Foundation and Contributors. See THIRD-PARTY-NOTICES.md.
pub(super) const LEAP_MONTHS: [u8; 200] = [
    0, 0, 5, 0, 0, 4, 0, 0, 2, 0, 6, 0, 0, 5, 0, 0, 2, 0, 7, 0, 0, 5, 0, 0, 4, 0, 0, 2, 0, 6, 0, 0,
    5, 0, 0, 3, 0, 7, 0, 0, 6, 0, 0, 4, 0, 0, 2, 0, 7, 0, 0, 5, 0, 0, 3, 0, 8, 0, 0, 6, 0, 0, 4, 0,
    0, 3, 0, 7, 0, 0, 5, 0, 0, 4, 0, 8, 0, 0, 6, 0, 0, 4, 0, 10, 0, 0, 6, 0, 0, 5, 0, 0, 3, 0, 8,
    0, 0, 5, 0, 0, 4, 0, 0, 2, 0, 7, 0, 0, 5, 0, 0, 4, 0, 9, 0, 0, 6, 0, 0, 4, 0, 0, 2, 0, 6, 0, 0,
    5, 0, 0, 3, 0, 11, 0, 0, 6, 0, 0, 5, 0, 0, 2, 0, 7, 0, 0, 5, 0, 0, 3, 0, 8, 0, 0, 6, 0, 0, 4,
    0, 0, 3, 0, 7, 0, 0, 5, 0, 0, 4, 0, 8, 0, 0, 6, 0, 0, 4, 0, 0, 3, 0, 7, 0, 0, 5, 0, 0, 4, 0, 8,
    0, 0, 6, 0, 0, 4, 0, 0, 2, 0,
];
